use super::{
    Arc, BTreeMap, Builtin, Callee, Catalog, Fault, Function, Global, Location, ModuleIdentity,
    Namespace, Op, Program, Site, SourceResolver, Value, decode_source, normalize_module,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

mod assignments;
mod calls;
mod expressions;
mod lexer;
mod statements;
use lexer::{Token, lex};

#[derive(Clone, Copy, PartialEq, Eq)]
enum CallSite {
    Call(Namespace),
    Spawn,
    Reference,
}

struct Pending {
    module: String,
    name: String,
    site: CallSite,
    statement: Option<usize>,
}

#[derive(Default)]
struct Tables {
    symbols: Vec<Arc<str>>,
    symbol_ids: BTreeMap<Arc<str>, u32>,
    pending: Vec<Pending>,
}

struct Parser {
    tables: Tables,
    slots: BTreeMap<String, u32>,
    module: String,
    function: String,
    tokens: Vec<Token>,
    pos: usize,
    code: Vec<(Location, Op)>,
    includes: Vec<String>,
    dependencies: BTreeSet<String>,
    loops: Vec<(Vec<usize>, Option<Vec<usize>>)>,
    constants: BTreeMap<String, Value>,
    animtree: Option<String>,
    single_byte_source: bool,
}
impl Parser {
    fn string_value(&self, text: &str) -> Value {
        if self.single_byte_source {
            Value::byte_string(&text.chars().map(|ch| ch as u8).collect::<Vec<_>>())
        } else {
            Value::string(text)
        }
    }

    fn location(&self) -> Location {
        let t = &self.tokens[self.pos];
        Location {
            module: self.module.clone(),
            function: self.function.clone(),
            line: t.line,
            column: t.column,
        }
    }
    fn error(&self, message: impl Into<String>) -> Fault {
        Fault::at(&self.location(), message)
    }
    fn is(&self, text: &str) -> bool {
        self.tokens[self.pos].is(text)
    }
    fn eat(&mut self, text: &str) -> bool {
        if self.is(text) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, text: &str) -> Result<(), Fault> {
        if self.eat(text) {
            Ok(())
        } else {
            Err(self.error(format!(
                "expected {text:?}, found {:?}",
                self.tokens[self.pos].text
            )))
        }
    }
    fn ident(&mut self) -> Result<String, Fault> {
        let t = &self.tokens[self.pos];
        if t.string
            || !t
                .text
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            return Err(self.error("expected identifier"));
        }
        let text = t.text.clone();
        self.pos += 1;
        Ok(text)
    }
    fn symbol(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.tables.symbol_ids.get(name) {
            return id;
        }
        let id = self.tables.symbols.len() as u32;
        let name: Arc<str> = name.into();
        self.tables.symbols.push(name.clone());
        self.tables.symbol_ids.insert(name, id);
        id
    }
    fn slot(&mut self, name: &str) -> u32 {
        let next = self.slots.len() as u32;
        *self.slots.entry(name.to_owned()).or_insert(next)
    }
    fn unlinked(&mut self, name: String, site: CallSite) -> u32 {
        self.tables.pending.push(Pending {
            module: self.module.clone(),
            name,
            site,
            statement: None,
        });
        (self.tables.pending.len() - 1) as u32
    }
    fn load(&mut self, name: &str) -> Op {
        match name {
            "self" => Op::Global(Global::SelfRef),
            "level" => Op::Global(Global::Level),
            "game" => Op::Global(Global::Game),
            "anim" => Op::Global(Global::Anim),
            _ => Op::Load(self.slot(name)),
        }
    }
    fn emit(&mut self, op: Op) -> usize {
        let pc = self.code.len();
        self.code.push((self.location(), op));
        pc
    }
    fn patch(&mut self, pc: usize, target: usize) {
        match &mut self.code[pc].1 {
            Op::Jump(to) | Op::JumpFalse(to) => *to = target,
            _ => unreachable!(),
        }
    }
    fn constant(&mut self, name: String) -> Result<(), Fault> {
        let start = self.code.len();
        self.expression(0)?;
        let code = self.code.split_off(start);
        let mut values = Vec::new();
        for (location, op) in code {
            let pop = |values: &mut Vec<Value>| {
                values
                    .pop()
                    .ok_or_else(|| Fault::at(&location, "invalid constant expression"))
            };
            match op {
                Op::Constant(value) => values.push(value),
                Op::Unary(op) => {
                    let value = pop(&mut values)?;
                    values.push(
                        super::runtime::unary(op, value).map_err(|e| Fault::at(&location, e))?,
                    );
                }
                Op::Binary(op) => {
                    let b = pop(&mut values)?;
                    let a = pop(&mut values)?;
                    values.push(
                        super::runtime::binary(op, a, b).map_err(|e| Fault::at(&location, e))?,
                    );
                }
                _ => {
                    return Err(Fault::at(
                        &location,
                        "constant requires a literal expression",
                    ));
                }
            }
        }
        if values.len() != 1 {
            return Err(self.error("invalid constant expression"));
        }
        if self.constants.insert(name, values.pop().unwrap()).is_some() {
            return Err(self.error("duplicate constant"));
        }
        Ok(())
    }
    fn module(&mut self) -> Result<Vec<Function>, Fault> {
        let mut functions = Vec::new();
        while !self.is("") {
            if self.eat("#") {
                if self.eat("define") {
                    let name = self.ident()?.to_ascii_lowercase();
                    let line = self.tokens[self.pos - 1].line;
                    if let Some(end) = (self.pos..self.tokens.len())
                        .find(|&i| self.tokens[i].line > line || self.tokens[i].is(""))
                    {
                        let mut separator = self.tokens[end].clone();
                        separator.text = ";".into();
                        separator.string = false;
                        self.tokens.insert(end, separator);
                    }
                    self.constant(name)?;
                    self.eat(";");
                    continue;
                }
                if self.eat("using_animtree") {
                    self.expect("(")?;
                    let token = self.tokens[self.pos].clone();
                    if !token.string {
                        return Err(self.error("expected animation tree string"));
                    }
                    self.pos += 1;
                    self.animtree = Some(token.text);
                    self.expect(")")?;
                    self.expect(";")?;
                    continue;
                }
                self.expect("include")?;
                let name = self.ident()?;
                let module = normalize_module(&name).map_err(|m| self.error(m))?;
                self.includes.push(module.clone());
                self.dependencies.insert(module);
                self.expect(";")?;
                continue;
            }
            self.function = self.ident()?.to_ascii_lowercase();
            if self.eat("=") {
                self.constant(self.function.clone())?;
                self.expect(";")?;
                continue;
            }
            let location = self.location();
            self.expect("(")?;
            self.slots.clear();
            let mut parameters = 0;
            if !self.is(")") {
                loop {
                    let parameter = self.ident()?.to_ascii_lowercase();
                    if ["self", "level", "game"].contains(&parameter.as_str()) {
                        return Err(self.error("reserved parameter name"));
                    }
                    if self.slots.contains_key(&parameter) {
                        // Scripts repeat parameter names; the name keeps its first slot.
                        self.slot(&format!("\0{parameters}"));
                    } else {
                        self.slot(&parameter);
                    }
                    parameters += 1;
                    if !self.eat(",") {
                        break;
                    }
                }
            }
            self.expect(")")?;
            if !self.is("{") {
                return Err(self.error("expected function body"));
            }
            self.statement()?;
            self.emit(Op::Constant(Value::Undefined));
            self.emit(Op::Return);
            functions.push(Function {
                location,
                parameters,
                slots: self.slots.len(),
                code: std::mem::take(&mut self.code),
            });
        }
        Ok(functions)
    }
}

pub(super) fn compile(
    resolver: &impl SourceResolver,
    roots: &[&str],
    catalog: &Catalog,
) -> Result<Program, Fault> {
    let root_location = Location {
        module: "<loader>".into(),
        function: String::new(),
        line: 1,
        column: 1,
    };
    if roots.is_empty() {
        return Err(Fault::at(&root_location, "no script roots"));
    }
    let mut pending = BTreeSet::new();
    for root in roots {
        pending.insert(normalize_module(root).map_err(|m| Fault::at(&root_location, m))?);
    }
    let mut functions = Vec::new();
    let mut names = BTreeMap::new();
    let mut modules = Vec::new();
    let mut tables = Tables::default();
    let mut imports = BTreeMap::new();
    while let Some(module) = pending.pop_first() {
        if imports.contains_key(&module) {
            continue;
        }
        let location = Location {
            module: module.clone(),
            ..root_location.clone()
        };
        let bytes = resolver
            .read_bytes(&module)
            .map_err(|m| Fault::at(&location, m))?;
        let source = decode_source(&bytes);
        let mut parser = Parser {
            tables: std::mem::take(&mut tables),
            slots: BTreeMap::new(),
            module: module.clone(),
            function: String::new(),
            tokens: lex(&module, &source)?,
            pos: 0,
            code: Vec::new(),
            includes: Vec::new(),
            dependencies: BTreeSet::new(),
            loops: Vec::new(),
            constants: BTreeMap::new(),
            animtree: None,
            single_byte_source: std::str::from_utf8(&bytes).is_err(),
        };
        for function in parser.module()? {
            let key = format!("{}::{}", module, function.location.function);
            if names.insert(key, functions.len()).is_some() {
                return Err(Fault::at(&function.location, "duplicate function"));
            }
            functions.push(function);
        }
        tables = parser.tables;
        imports.insert(module.clone(), parser.includes);
        pending.extend(parser.dependencies);
        modules.push(ModuleIdentity {
            site: Site::Server,
            realm: catalog.realm(),
            origin: resolver.origin(&module),
            module,
            sha256: Sha256::digest(&bytes).into(),
        });
    }
    let mut native_ids = BTreeMap::new();
    let mut natives = Vec::new();
    let mut resolved = Vec::with_capacity(tables.pending.len());
    for pending in &tables.pending {
        resolved.push(link(pending, &names, &imports, catalog).map(|link| {
            match link {
                Link::Builtin(builtin) => Link::Callee(Callee::Native(
                    *native_ids
                        .entry((builtin.namespace, builtin.name))
                        .or_insert_with(|| {
                            natives.push(builtin.clone());
                            natives.len() as u32 - 1
                        }),
                )),
                other => other,
            }
        }));
    }
    for function in &mut functions {
        let mut elided = Vec::new();
        for (pc, (_, op)) in function.code.iter().enumerate() {
            if let Op::Call(Callee::Unlinked(id), ..) = op
                && let Ok(Link::Elide) = resolved[*id as usize]
            {
                elided.push(tables.pending[*id as usize].statement.unwrap()..pc + 2);
            }
        }
        if !elided.is_empty() {
            let mut keep = vec![true; function.code.len()];
            for range in elided {
                keep[range].fill(false);
            }
            let mut remap = Vec::with_capacity(keep.len() + 1);
            let mut next = 0;
            for &kept in &keep {
                remap.push(next);
                next += usize::from(kept);
            }
            remap.push(next);
            let mut kept = keep.iter();
            function.code.retain(|_| *kept.next().unwrap());
            for (_, op) in &mut function.code {
                if let Op::Jump(to) | Op::JumpFalse(to) = op {
                    *to = remap[*to];
                }
            }
        }
        for (location, op) in &mut function.code {
            let (id, argc) = match op {
                Op::Call(Callee::Unlinked(id), argc, _)
                | Op::Spawn(Callee::Unlinked(id), argc, _) => (*id, *argc),
                Op::FunctionRef(id) => (*id, 0),
                _ => continue,
            };
            let callee = match resolved[id as usize]
                .clone()
                .map_err(|m| Fault::at(location, m))?
            {
                Link::Callee(callee) => callee,
                Link::Builtin(_) | Link::Elide => unreachable!(),
            };
            if matches!(callee, Callee::Native(_)) && argc >= 256 {
                return Err(Fault::at(location, "parameter count exceeds 256"));
            }
            match op {
                Op::Call(slot, ..) | Op::Spawn(slot, ..) => *slot = callee,
                _ => {
                    *op = Op::Constant(match callee {
                        Callee::Script(target) => Value::Function(target),
                        Callee::Native(target) => Value::Builtin(target),
                        Callee::Unlinked(_) => unreachable!(),
                    })
                }
            }
        }
    }
    modules.sort_by(|a, b| a.module.cmp(&b.module));
    Ok(Program {
        impure_scripts: modules
            .iter()
            .any(|module| module.origin == super::SourceOrigin::External),
        functions,
        names,
        modules,
        symbols: tables.symbols,
        symbol_ids: tables.symbol_ids,
        natives,
        rules: catalog.realm(),
    })
}

#[derive(Clone)]
enum Link {
    Callee(Callee),
    Builtin(Builtin),
    Elide,
}

fn link(
    pending: &Pending,
    names: &BTreeMap<String, usize>,
    imports: &BTreeMap<String, Vec<String>>,
    catalog: &Catalog,
) -> Result<Link, String> {
    let Pending {
        module,
        name,
        site,
        statement,
    } = pending;
    let script = |key: &str| {
        names
            .get(key)
            .map(|&id| Link::Callee(Callee::Script(id as u32)))
    };
    if name.contains("::") {
        return script(name).ok_or_else(|| format!("unresolved function {name}"));
    }
    if let Some(local) = script(&format!("{module}::{name}")) {
        return Ok(local);
    }
    let builtin = match site {
        CallSite::Call(namespace) => catalog.get(*namespace, name),
        CallSite::Reference => catalog
            .get(Namespace::Function, name)
            .or_else(|| catalog.get(Namespace::Method, name)),
        CallSite::Spawn => None,
    };
    if let Some(builtin) = builtin {
        return match (builtin.developer, site, statement) {
            (false, ..) => Ok(Link::Builtin(builtin.clone())),
            (true, CallSite::Call(_), Some(_)) => Ok(Link::Elide),
            (true, CallSite::Call(_), None) => Err(format!(
                "return value of developer builtin {name} is only accessible in a developer block"
            )),
            (true, ..) => Err(format!("reference to developer builtin {name}")),
        };
    }
    let mut candidates = imports[module]
        .iter()
        .filter_map(|m| names.get(&format!("{m}::{name}")))
        .collect::<BTreeSet<_>>()
        .into_iter();
    match (candidates.next(), candidates.next()) {
        (Some(&id), None) => Ok(Link::Callee(Callee::Script(id as u32))),
        (Some(_), Some(_)) => Err(format!("ambiguous imported function {name}")),
        (None, _) => Err(match site {
            CallSite::Spawn => format!("unresolved thread function {name}"),
            CallSite::Call(Namespace::Method) => {
                format!("unresolved method or unknown builtin {name}")
            }
            _ => format!("unresolved function or unknown builtin {name}"),
        }),
    }
}
