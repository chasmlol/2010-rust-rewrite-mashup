use super::Parser;
use crate::script::{Binary, Callee, Fault, Op, Value};

impl Parser {
    pub(super) fn statement(&mut self) -> Result<(), Fault> {
        if self.eat("{") {
            while !self.eat("}") {
                if self.is("") {
                    return Err(self.error("unterminated block"));
                }
                self.statement()?;
            }
        } else if self.eat(";") {
        } else if self.eat("if") {
            self.expect("(")?;
            self.expression(0)?;
            self.expect(")")?;
            let branch = self.emit(Op::JumpFalse(0));
            self.statement()?;
            if self.eat("else") {
                let done = self.emit(Op::Jump(0));
                self.patch(branch, self.code.len());
                self.statement()?;
                self.patch(done, self.code.len());
            } else {
                self.patch(branch, self.code.len());
            }
        } else if self.eat("foreach") {
            self.expect("(")?;
            let first = self.ident()?.to_ascii_lowercase();
            let (key, value) = if self.eat(",") {
                (Some(first), self.ident()?.to_ascii_lowercase())
            } else {
                (None, first)
            };
            if key
                .iter()
                .chain(std::iter::once(&value))
                .any(|n| ["self", "level", "game"].contains(&n.as_str()))
            {
                return Err(self.error("cannot assign global receiver"));
            }
            self.expect("in")?;
            self.expression(0)?;
            self.expect(")")?;
            let array = self.slot(&format!("$array{}", self.pos));
            let keys = self.slot(&format!("$keys{}", self.pos));
            let cursor = self.slot(&format!("$cursor{}", self.pos));
            self.emit(Op::Dup);
            self.emit(Op::Store(array));
            self.emit(Op::ArrayKeys);
            self.emit(Op::Store(keys));
            self.emit(Op::Constant(Value::Int(0)));
            self.emit(Op::Store(cursor));
            let condition = self.code.len();
            self.emit(Op::Load(cursor));
            self.emit(Op::Load(keys));
            self.emit(Op::Size);
            self.emit(Op::Binary(Binary::Less));
            let done = self.emit(Op::JumpFalse(0));
            self.emit(Op::Load(array));
            self.emit(Op::Load(keys));
            self.emit(Op::Load(cursor));
            self.emit(Op::LoadIndex);
            if let Some(key) = key {
                let key = self.slot(&key);
                self.emit(Op::Dup);
                self.emit(Op::Store(key));
            }
            self.emit(Op::LoadIndex);
            let value = self.slot(&value);
            self.emit(Op::Store(value));
            self.loops.push((Vec::new(), Some(Vec::new())));
            self.statement()?;
            let increment = self.code.len();
            self.emit(Op::Load(cursor));
            self.emit(Op::Constant(Value::Int(1)));
            self.emit(Op::Binary(Binary::Add));
            self.emit(Op::Store(cursor));
            self.emit(Op::Jump(condition));
            let end = self.code.len();
            self.patch(done, end);
            let (breaks, continues) = self.loops.pop().unwrap();
            for pc in breaks {
                self.patch(pc, end);
            }
            for pc in continues.unwrap() {
                self.patch(pc, increment);
            }
        } else if self.eat("switch") {
            self.expect("(")?;
            self.expression(0)?;
            self.expect(")")?;
            let selector = self.slot(&format!("$switch{}", self.pos));
            self.emit(Op::Store(selector));
            self.expect("{")?;
            let dispatch = self.emit(Op::Jump(0));
            let mut cases = Vec::new();
            let mut default = None;
            self.loops.push((Vec::new(), None));
            while !self.eat("}") {
                if self.eat("case") {
                    let negative = self.eat("-");
                    let t = self.tokens[self.pos].clone();
                    if negative && t.string {
                        return Err(self.error("case label must be an integer or string"));
                    }
                    let value = if t.string {
                        self.pos += 1;
                        self.string_value(&t.text)
                    } else {
                        let text = &self.tokens[self.pos].text;
                        let text = if negative {
                            format!("-{text}")
                        } else {
                            text.clone()
                        };
                        let value = match self.number(&text) {
                            Some(Ok(value @ Value::Int(_))) => value,
                            _ => return Err(self.error("case label must be an integer or string")),
                        };
                        self.pos += 1;
                        value
                    };
                    if cases.iter().any(|(v, _)| *v == value) {
                        return Err(self.error("duplicate case label"));
                    }
                    self.expect(":")?;
                    cases.push((value, self.code.len()));
                } else if self.eat("default") {
                    self.expect(":")?;
                    if default.replace(self.code.len()).is_some() {
                        return Err(self.error("duplicate default"));
                    }
                } else {
                    if self.is("") {
                        return Err(self.error("unterminated switch"));
                    }
                    self.statement()?;
                }
            }
            let finish = self.emit(Op::Jump(0));
            self.patch(dispatch, self.code.len());
            for (value, target) in cases {
                self.emit(Op::Load(selector));
                self.emit(Op::Constant(value));
                self.emit(Op::Binary(Binary::Case));
                let next = self.emit(Op::JumpFalse(0));
                self.emit(Op::Jump(target));
                self.patch(next, self.code.len());
            }
            let otherwise = self.emit(Op::Jump(default.unwrap_or(0)));
            let end = self.code.len();
            if default.is_none() {
                self.patch(otherwise, end);
            }
            self.patch(finish, end);
            let (breaks, _) = self.loops.pop().unwrap();
            for pc in breaks {
                self.patch(pc, end);
            }
        } else if self.eat("for") {
            self.expect("(")?;
            if !self.is(";") {
                self.assignment()?;
            }
            self.expect(";")?;
            let condition = self.code.len();
            if self.is(";") {
                self.emit(Op::Constant(Value::Int(1)));
            } else {
                self.expression(0)?;
            }
            self.expect(";")?;
            let done = self.emit(Op::JumpFalse(0));
            let body = self.emit(Op::Jump(0));
            let increment = self.code.len();
            if !self.is(")") {
                self.assignment()?;
            }
            self.expect(")")?;
            self.emit(Op::Jump(condition));
            self.patch(body, self.code.len());
            self.loops.push((Vec::new(), Some(Vec::new())));
            self.statement()?;
            self.emit(Op::Jump(increment));
            let end = self.code.len();
            self.patch(done, end);
            let (breaks, continues) = self.loops.pop().unwrap();
            for pc in breaks {
                self.patch(pc, end);
            }
            for pc in continues.unwrap() {
                self.patch(pc, increment);
            }
        } else if self.eat("while") {
            let condition = self.code.len();
            self.expect("(")?;
            self.expression(0)?;
            self.expect(")")?;
            let done = self.emit(Op::JumpFalse(0));
            self.loops.push((Vec::new(), Some(Vec::new())));
            self.statement()?;
            self.emit(Op::Jump(condition));
            let end = self.code.len();
            self.patch(done, end);
            let (breaks, continues) = self.loops.pop().unwrap();
            for pc in breaks {
                self.patch(pc, end);
            }
            for pc in continues.unwrap() {
                self.patch(pc, condition);
            }
        } else if self.is("break") || self.is("continue") {
            let is_break = self.eat("break");
            if !is_break {
                self.expect("continue")?;
            }
            if self.loops.is_empty() {
                return Err(self.error("break/continue outside loop"));
            }
            let pc = self.emit(Op::Jump(0));
            if is_break {
                self.loops.last_mut().unwrap().0.push(pc);
            } else {
                self.loops
                    .iter_mut()
                    .rev()
                    .find_map(|(_, c)| c.as_mut())
                    .ok_or_else(|| Fault::at(&self.code[pc].0, "continue outside loop"))?
                    .push(pc);
            }
            self.expect(";")?;
        } else if self.eat("return") {
            if self.is(";") {
                self.emit(Op::Constant(Value::Undefined));
            } else {
                self.expression(0)?;
            }
            self.emit(Op::Return);
            self.expect(";")?;
        } else if self.eat("waittillframeend") {
            self.emit(Op::FrameEnd);
            self.expect(";")?;
        } else if self.eat("wait") {
            self.expression(0)?;
            self.emit(Op::Wait);
            self.expect(";")?;
        } else if self.assignment_ahead() {
            self.assignment()?;
            self.expect(";")?;
        } else if (self.is("prof_begin") || self.is("prof_end"))
            && self.tokens.get(self.pos + 1).is_some_and(|t| t.is("("))
        {
            self.pos += 2;
            if !self.tokens[self.pos].string {
                return Err(self.error("expected profile name string"));
            }
            self.pos += 1;
            self.expect(")")?;
            self.expect(";")?;
        } else {
            let start = self.code.len();
            self.expression(0)?;
            if let Some((_, Op::Call(Callee::Unlinked(id), ..))) = self.code.last() {
                self.tables.pending[*id as usize].statement = Some(start);
            }
            self.emit(Op::Pop);
            self.expect(";")?;
        }
        Ok(())
    }
}
