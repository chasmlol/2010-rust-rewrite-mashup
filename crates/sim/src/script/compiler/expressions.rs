use super::{CallSite, Parser};
use crate::script::{Binary, Callee, Fault, Namespace, Op, Unary, Value};

impl Parser {
    pub(super) fn number(&self, text: &str) -> Option<Result<Value, Fault>> {
        let digits = text.strip_prefix('-').unwrap_or(text);
        if !digits.starts_with(|c: char| c.is_ascii_digit() || c == '.')
            || !digits.bytes().any(|b| b.is_ascii_digit())
        {
            return None;
        }
        Some(if text.contains('.') {
            match text.parse::<f32>() {
                Ok(n) if n.is_finite() => Ok(Value::Float(n)),
                _ => Err(self.error("invalid float literal")),
            }
        } else if !digits.bytes().all(|b| b.is_ascii_digit()) {
            Err(self.error("invalid integer literal"))
        } else {
            // Long literals wrap to 32 bits, never saturate.
            let n = digits.bytes().fold(0u32, |n, b| {
                n.wrapping_mul(10).wrapping_add(u32::from(b - b'0'))
            }) as i32;
            Ok(Value::Int(if text.starts_with('-') {
                n.wrapping_neg()
            } else {
                n
            }))
        })
    }

    pub(super) fn expression(&mut self, min: u8) -> Result<(), Fault> {
        let location = self.location();
        if self.eat("call") {
            self.invocation(false, false)?;
        } else if self.eat("thread") {
            self.invocation(false, true)?;
        } else if self.eat("%") {
            let name = self.ident()?.to_ascii_lowercase();
            let tree = self
                .animtree
                .clone()
                .ok_or_else(|| self.error("animation literal without an animation tree"))?;
            self.emit(Op::Constant(Value::Animation {
                tree: tree.into(),
                name: name.into(),
            }));
        } else if self.eat("#") {
            if self.tokens[self.pos].string {
                let text = self.tokens[self.pos].text.clone();
                self.pos += 1;
                self.emit(Op::Constant(self.string_value(&text)));
            } else {
                self.expect("animtree")?;
                let tree = self
                    .animtree
                    .clone()
                    .ok_or_else(|| self.error("animation tree is not declared"))?;
                self.emit(Op::Constant(Value::AnimationTree(tree.into())));
            }
        } else if self.eat("::") {
            let name = self.ident()?.to_ascii_lowercase();
            let id = self.unlinked(name, CallSite::Reference);
            self.emit(Op::FunctionRef(id));
        } else if self.eat("&") {
            let t = self.tokens[self.pos].clone();
            if !t.string {
                return Err(self.error("expected localized string"));
            }
            self.pos += 1;
            self.emit(Op::Constant(Value::LocalizedString(t.text.into())));
        } else if self.eat("[") {
            if self.eat("[") {
                self.expression(0)?;
                self.expect("]")?;
                self.expect("]")?;
                let argc = self.arguments()?;
                self.emit(Op::Indirect(argc, false, false));
            } else {
                self.expect("]")?;
                self.emit(Op::Array);
            }
        } else if self.eat("~") {
            self.expression(12)?;
            self.emit(Op::Unary(Unary::Complement));
        } else if self.eat("!") {
            self.expression(12)?;
            self.emit(Op::Unary(Unary::Not));
        } else if self.eat("-") {
            let t = &self.tokens[self.pos];
            let literal = (!t.string)
                .then(|| self.number(&format!("-{}", t.text)))
                .flatten();
            if let Some(value) = literal {
                let value = value?;
                self.pos += 1;
                self.emit(Op::Constant(value));
            } else {
                self.emit(Op::Constant(Value::Int(0)));
                self.expression(12)?;
                self.emit(Op::Binary(Binary::Sub));
            }
        } else if self.eat("(") {
            self.expression(0)?;
            if self.eat(",") {
                self.expression(0)?;
                self.expect(",")?;
                self.expression(0)?;
                self.emit(Op::Vector);
            }
            self.expect(")")?;
        } else {
            let t = self.tokens[self.pos].clone();
            if t.string {
                self.pos += 1;
                self.emit(Op::Constant(self.string_value(&t.text)));
            } else if let Some(value) = self.number(&t.text) {
                let value = value?;
                self.pos += 1;
                self.emit(Op::Constant(value));
            } else if self.eat("undefined") {
                self.emit(Op::Constant(Value::Undefined));
            } else if self.eat("true") {
                self.emit(Op::Constant(Value::Int(1)));
            } else if self.eat("false") {
                self.emit(Op::Constant(Value::Int(0)));
            } else {
                let first = self.ident()?;
                let name = self.call_name(first)?;
                if self.is("(") {
                    let site = CallSite::Call(Namespace::Function);
                    let callee = Callee::Unlinked(self.unlinked(name, site));
                    let argc = self.arguments()?;
                    self.code
                        .push((location.clone(), Op::Call(callee, argc, false)));
                } else if name.contains("::") || name.contains('\\') {
                    let id = self.unlinked(name, CallSite::Reference);
                    self.emit(Op::FunctionRef(id));
                } else if let Some(value) = self.constants.get(&name).cloned() {
                    self.emit(Op::Constant(value));
                } else {
                    let op = self.load(&name);
                    self.emit(op);
                }
            }
        }
        loop {
            if self.eat(".") {
                let field = self.ident()?.to_ascii_lowercase();
                if field == "size" {
                    self.emit(Op::Size);
                } else {
                    let field = self.symbol(&field);
                    self.emit(Op::LoadField(field));
                }
            } else if self.is("[") && !self.tokens.get(self.pos + 1).is_some_and(|t| t.is("[")) {
                self.pos += 1;
                self.expression(0)?;
                self.expect("]")?;
                self.emit(Op::LoadIndex);
            } else if self.eat("call") {
                self.invocation(true, false)?;
            } else if self.eat("thread") {
                self.invocation(true, true)?;
            } else {
                let receiver_call = !self.tokens[self.pos].string
                    && self.tokens[self.pos]
                        .text
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && self
                        .tokens
                        .get(self.pos + 1)
                        .is_some_and(|t| t.is("(") || t.is("::"));
                if receiver_call
                    || (self.is("[") && self.tokens.get(self.pos + 1).is_some_and(|t| t.is("[")))
                {
                    self.invocation(true, false)?;
                } else {
                    break;
                }
            }
        }
        loop {
            if self.tokens[self.pos].string {
                break;
            }
            let op = self.tokens[self.pos].text.clone();
            let (power, binary) = match op.as_str() {
                "||" => (1, None),
                "&&" => (2, None),
                "|" => (3, Some(Binary::Or)),
                "^" => (4, Some(Binary::Xor)),
                "&" => (5, Some(Binary::And)),
                "==" => (6, Some(Binary::Equal)),
                "!=" => (6, Some(Binary::NotEqual)),
                "<" => (7, Some(Binary::Less)),
                ">" => (7, Some(Binary::Greater)),
                "<=" => (7, Some(Binary::LessEqual)),
                ">=" => (7, Some(Binary::GreaterEqual)),
                "<<" => (8, Some(Binary::Shl)),
                ">>" => (8, Some(Binary::Shr)),
                "+" => (9, Some(Binary::Add)),
                "-" => (9, Some(Binary::Sub)),
                "*" => (10, Some(Binary::Mul)),
                "/" => (10, Some(Binary::Div)),
                "%" => (10, Some(Binary::Mod)),
                _ => break,
            };
            if power < min {
                break;
            }
            self.pos += 1;
            if let Some(binary) = binary {
                self.expression(power + 1)?;
                self.code.push((location.clone(), Op::Binary(binary)));
            } else {
                self.emit(Op::Unary(Unary::Not));
                self.emit(Op::Unary(Unary::Not));
                if op == "||" {
                    self.emit(Op::Unary(Unary::Not));
                }
                let branch = self.emit(Op::JumpFalse(0));
                self.expression(power + 1)?;
                self.emit(Op::Unary(Unary::Not));
                self.emit(Op::Unary(Unary::Not));
                let done = self.emit(Op::Jump(0));
                self.patch(branch, self.code.len());
                self.emit(Op::Constant(Value::Int(i32::from(op == "||"))));
                self.patch(done, self.code.len());
            }
        }
        if min == 0 && self.eat("?") {
            let alternative = self.emit(Op::JumpFalse(0));
            self.expression(0)?;
            self.expect(":")?;
            let done = self.emit(Op::Jump(0));
            self.patch(alternative, self.code.len());
            self.expression(0)?;
            self.patch(done, self.code.len());
        }
        Ok(())
    }
}
