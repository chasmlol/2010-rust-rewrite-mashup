use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

const SOURCE: &str = include_str!("../schema/t6.sexp");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Xor,
    Or,
    Shl,
    Shr,
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
    Ne,
    LogicAnd,
    LogicOr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScalarKind {
    Signed,
    Unsigned,
    Float,
}

#[derive(Debug)]
pub enum Eval {
    Num(i64),
    Op(BinOp, Box<Eval>, Box<Eval>),
    Var {
        var: usize,
        steps: Vec<(u32, bool)>,
        indices: Vec<(Eval, u32)>,
        size: u8,
        kind: ScalarKind,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartLoad {
    Full,
    Partial(u32),
    None,
}

#[derive(Debug)]
pub enum Op {
    Push(u8),
    Pop,
    If(Eval, Vec<Op>),
    Chain(Vec<(Option<Eval>, Vec<Op>)>),
    Block(u8, Vec<Op>),
    IfNonZero(u32, Vec<Op>),
    Reuse {
        off: u32,
        temp: bool,
        body: Vec<Op>,
    },
    Alloc {
        off: u32,
        align: Eval,
        temp: bool,
        body: Vec<Op>,
    },
    XString(u32),
    XStringArray {
        off: u32,
        deref: bool,
        start: bool,
        count: Eval,
    },
    AssetLoad {
        off: u32,
        asset: usize,
    },
    PtrArray {
        off: u32,
        deref: bool,
        func: usize,
        start: bool,
        count: Eval,
    },
    Array {
        off: u32,
        deref: bool,
        func: usize,
        start: bool,
        count: Eval,
    },
    Single {
        off: u32,
        load: usize,
    },
    Embedded {
        off: u32,
        load: usize,
        start: bool,
    },
    Raw {
        off: u32,
        deref: bool,
        elem: u32,
        count: Eval,
    },
    Nop,
}

#[derive(Debug)]
pub struct LoadFn {
    pub name: String,
    pub size: u32,
    pub start: StartLoad,
    pub var: usize,
    pub body: Vec<Op>,
}

#[derive(Debug)]
pub struct ArrayFn {
    pub size: u32,
    pub load: usize,
}

#[derive(Debug)]
pub struct PtrArrayFn {
    pub size: u32,
    pub load: Option<usize>,
    pub reusable: bool,
    pub asset: Option<usize>,
    pub align: Eval,
}

#[derive(Debug)]
pub struct AssetSchema {
    pub name: String,
    pub loads: Vec<LoadFn>,
    pub arrays: Vec<ArrayFn>,
    pub ptr_arrays: Vec<PtrArrayFn>,
    pub root: usize,
    pub root_in_temp: bool,
    pub root_align: Eval,
    pub var_count: usize,
}

#[derive(Debug)]
pub struct Schema {
    pub assets: Vec<AssetSchema>,
}

impl Schema {
    pub fn asset_by_name(&self, name: &str) -> Option<usize> {
        self.assets.iter().position(|a| a.name == name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SchemaError {
    pub at: usize,
    pub what: &'static str,
}

#[derive(Debug)]
enum Sx<'a> {
    Atom(&'a str, usize),
    List(Vec<Sx<'a>>, usize),
}

impl<'a> Sx<'a> {
    fn at(&self) -> usize {
        match self {
            Sx::Atom(_, at) | Sx::List(_, at) => *at,
        }
    }

    fn list(&self) -> Result<&[Sx<'a>], SchemaError> {
        match self {
            Sx::List(items, _) => Ok(items),
            Sx::Atom(_, at) => Err(SchemaError {
                at: *at,
                what: "expected a list",
            }),
        }
    }

    fn atom(&self) -> Result<&'a str, SchemaError> {
        match self {
            Sx::Atom(s, _) => Ok(s),
            Sx::List(_, at) => Err(SchemaError {
                at: *at,
                what: "expected an atom",
            }),
        }
    }

    fn head(&self) -> Result<&'a str, SchemaError> {
        self.list()?
            .first()
            .ok_or(SchemaError {
                at: self.at(),
                what: "empty list",
            })?
            .atom()
    }

    fn num<T: TryFrom<i64>>(&self) -> Result<T, SchemaError> {
        let at = self.at();
        let bad = SchemaError {
            at,
            what: "expected a number",
        };
        let n: i64 = self.atom()?.parse().map_err(|_| bad)?;
        T::try_from(n).map_err(|_| bad)
    }

    fn flag(&self) -> Result<bool, SchemaError> {
        Ok(self.num::<u8>()? != 0)
    }
}

fn tokenize(src: &str) -> Result<Vec<Sx<'_>>, SchemaError> {
    let bytes = src.as_bytes();
    let mut stack: Vec<(Vec<Sx<'_>>, usize)> = Vec::new();
    let mut top: Vec<Sx<'_>> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b';' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'(' => {
                stack.push((core::mem::take(&mut top), i));
                i += 1;
            }
            b')' => {
                let (parent, at) = stack.pop().ok_or(SchemaError {
                    at: i,
                    what: "unbalanced ')'",
                })?;
                let list = core::mem::replace(&mut top, parent);
                top.push(Sx::List(list, at));
                i += 1;
            }
            c if c.is_ascii_whitespace() => i += 1,
            _ => {
                let start = i;
                while i < bytes.len()
                    && !bytes[i].is_ascii_whitespace()
                    && !matches!(bytes[i], b'(' | b')')
                {
                    i += 1;
                }
                top.push(Sx::Atom(&src[start..i], start));
            }
        }
    }
    if let Some((_, at)) = stack.pop() {
        return Err(SchemaError {
            at,
            what: "unclosed '('",
        });
    }
    Ok(top)
}

#[derive(Default)]
struct Names<'a> {
    loads: Vec<&'a str>,
    arrays: Vec<&'a str>,
    ptr_arrays: Vec<&'a str>,
    vars: Vec<&'a str>,
}

fn index_of<'a>(table: &mut Vec<&'a str>, name: &'a str) -> usize {
    match table.iter().position(|n| *n == name) {
        Some(i) => i,
        None => {
            table.push(name);
            table.len() - 1
        }
    }
}

struct Parser<'a, 's> {
    names: &'s mut Names<'a>,
    asset_names: &'s [&'a str],
}

impl<'a> Parser<'a, '_> {
    fn asset(&self, sx: &Sx<'a>) -> Result<usize, SchemaError> {
        let name = sx.atom()?;
        self.asset_names
            .iter()
            .position(|n| *n == name)
            .ok_or(SchemaError {
                at: sx.at(),
                what: "unknown asset",
            })
    }

    fn eval(&mut self, sx: &Sx<'a>) -> Result<Eval, SchemaError> {
        let items = sx.list()?;
        let bad = SchemaError {
            at: sx.at(),
            what: "bad evaluation",
        };
        Ok(match sx.head()? {
            "n" => Eval::Num(items.get(1).ok_or(bad)?.num()?),
            "op" => {
                let op = match items.get(1).ok_or(bad)?.atom()? {
                    "+" => BinOp::Add,
                    "-" => BinOp::Sub,
                    "*" => BinOp::Mul,
                    "/" => BinOp::Div,
                    "%" => BinOp::Rem,
                    "&" => BinOp::And,
                    "^" => BinOp::Xor,
                    "|" => BinOp::Or,
                    "<<" => BinOp::Shl,
                    ">>" => BinOp::Shr,
                    ">" => BinOp::Gt,
                    ">=" => BinOp::Ge,
                    "<" => BinOp::Lt,
                    "<=" => BinOp::Le,
                    "==" => BinOp::Eq,
                    "!=" => BinOp::Ne,
                    "&&" => BinOp::LogicAnd,
                    "||" => BinOp::LogicOr,
                    _ => return Err(bad),
                };
                let a = self.eval(items.get(2).ok_or(bad)?)?;
                let b = self.eval(items.get(3).ok_or(bad)?)?;
                Eval::Op(op, Box::new(a), Box::new(b))
            }
            "v" => {
                let var = index_of(&mut self.names.vars, items.get(1).ok_or(bad)?.atom()?);
                let mut steps = Vec::new();
                let mut indices = Vec::new();
                let mut scalar = None;
                for part in &items[2..] {
                    let p = part.list()?;
                    match part.head()? {
                        "m" => {
                            steps.push((p.get(1).ok_or(bad)?.num()?, p.get(2).ok_or(bad)?.flag()?))
                        }
                        "i" => indices.push((
                            self.eval(p.get(1).ok_or(bad)?)?,
                            p.get(2).ok_or(bad)?.num()?,
                        )),
                        "t" => {
                            let size: u8 = p.get(1).ok_or(bad)?.num()?;
                            let kind = match p.get(2).ok_or(bad)?.atom()? {
                                "i" => ScalarKind::Signed,
                                "u" => ScalarKind::Unsigned,
                                "f" => ScalarKind::Float,
                                _ => return Err(bad),
                            };
                            scalar = Some((size, kind));
                        }
                        _ => return Err(bad),
                    }
                }
                let (size, kind) = scalar.ok_or(bad)?;
                Eval::Var {
                    var,
                    steps,
                    indices,
                    size,
                    kind,
                }
            }
            _ => return Err(bad),
        })
    }

    fn body(&mut self, items: &[Sx<'a>]) -> Result<Vec<Op>, SchemaError> {
        items.iter().map(|sx| self.op(sx)).collect()
    }

    fn op(&mut self, sx: &Sx<'a>) -> Result<Op, SchemaError> {
        let it = sx.list()?;
        let bad = SchemaError {
            at: sx.at(),
            what: "bad op",
        };
        let g = |i: usize| it.get(i).ok_or(bad);
        Ok(match sx.head()? {
            "push" => Op::Push(g(1)?.num()?),
            "pop" => Op::Pop,
            "nop" => Op::Nop,
            "if" => Op::If(self.eval(g(1)?)?, self.body(&it[2..])?),
            "chain" => {
                let mut cases = Vec::new();
                for case in &it[1..] {
                    let c = case.list()?;
                    let cond = c.get(1).ok_or(bad)?;
                    let cond = if cond.head()? == "else" {
                        None
                    } else {
                        Some(self.eval(cond)?)
                    };
                    cases.push((cond, self.body(&c[2..])?));
                }
                Op::Chain(cases)
            }
            "block" => Op::Block(g(1)?.num()?, self.body(&it[2..])?),
            "ifnz" => Op::IfNonZero(g(1)?.num()?, self.body(&it[2..])?),
            "reuse" => Op::Reuse {
                off: g(1)?.num()?,
                temp: g(2)?.flag()?,
                body: self.body(&it[3..])?,
            },
            "alloc" => Op::Alloc {
                off: g(1)?.num()?,
                align: self.eval(g(2)?)?,
                temp: g(3)?.flag()?,
                body: self.body(&it[4..])?,
            },
            "xstring" => Op::XString(g(1)?.num()?),
            "xstrarr" => Op::XStringArray {
                off: g(1)?.num()?,
                deref: g(2)?.flag()?,
                start: g(3)?.flag()?,
                count: self.eval(g(4)?)?,
            },
            "assetload" => Op::AssetLoad {
                off: g(1)?.num()?,
                asset: self.asset(g(2)?)?,
            },
            "ptrarr" => Op::PtrArray {
                off: g(1)?.num()?,
                deref: g(2)?.flag()?,
                func: index_of(&mut self.names.ptr_arrays, g(3)?.atom()?),
                start: g(4)?.flag()?,
                count: self.eval(g(5)?)?,
            },
            "arr" => Op::Array {
                off: g(1)?.num()?,
                deref: g(2)?.flag()?,
                func: index_of(&mut self.names.arrays, g(3)?.atom()?),
                start: g(4)?.flag()?,
                count: self.eval(g(5)?)?,
            },
            "single" => Op::Single {
                off: g(1)?.num()?,
                load: index_of(&mut self.names.loads, g(2)?.atom()?),
            },
            "embedded" => Op::Embedded {
                off: g(1)?.num()?,
                load: index_of(&mut self.names.loads, g(2)?.atom()?),
                start: g(3)?.flag()?,
            },
            "raw" => Op::Raw {
                off: g(1)?.num()?,
                deref: g(2)?.flag()?,
                elem: g(3)?.num()?,
                count: self.eval(g(4)?)?,
            },
            _ => return Err(bad),
        })
    }
}

fn missing(at: usize) -> SchemaError {
    SchemaError {
        at,
        what: "referenced function is not defined in this asset",
    }
}

fn pad<T>(v: &mut Vec<Option<T>>, n: usize) {
    if v.len() < n {
        v.resize_with(n, || None);
    }
}

pub fn parse() -> Result<Schema, SchemaError> {
    let top = tokenize(SOURCE)?;
    let asset_names: Vec<&str> = top
        .iter()
        .map(|a| a.list()?.get(1).ok_or(missing(a.at()))?.atom())
        .collect::<Result<_, _>>()?;

    let mut assets = Vec::with_capacity(top.len());
    for (asset_sx, &asset_name) in top.iter().zip(&asset_names) {
        let mut names = Names::default();
        let mut p = Parser {
            names: &mut names,
            asset_names: &asset_names,
        };
        let mut loads: Vec<Option<LoadFn>> = Vec::new();
        let mut arrays: Vec<Option<ArrayFn>> = Vec::new();
        let mut ptr_arrays: Vec<Option<PtrArrayFn>> = Vec::new();
        let mut root = None;

        for item in &asset_sx.list()?[2..] {
            let it = item.list()?;
            let bad = SchemaError {
                at: item.at(),
                what: "bad asset item",
            };
            let g = |i: usize| it.get(i).ok_or(bad);
            match item.head()? {
                "load" => {
                    let name = g(1)?.atom()?;
                    let idx = index_of(&mut p.names.loads, name);
                    let var = index_of(&mut p.names.vars, name);
                    let start_sx = g(3)?;
                    let start = match start_sx.head()? {
                        "full" => StartLoad::Full,
                        "partial" => StartLoad::Partial(start_sx.list()?.get(1).ok_or(bad)?.num()?),
                        "none" => StartLoad::None,
                        _ => return Err(bad),
                    };
                    let f = LoadFn {
                        name: name.into(),
                        size: g(2)?.num()?,
                        start,
                        var,
                        body: p.body(&it[4..])?,
                    };
                    if loads.len() <= idx {
                        loads.resize_with(idx + 1, || None);
                    }
                    loads[idx] = Some(f);
                }
                "array" => {
                    let idx = index_of(&mut p.names.arrays, g(1)?.atom()?);
                    let f = ArrayFn {
                        size: g(2)?.num()?,
                        load: index_of(&mut p.names.loads, g(3)?.atom()?),
                    };
                    if arrays.len() <= idx {
                        arrays.resize_with(idx + 1, || None);
                    }
                    arrays[idx] = Some(f);
                }
                "ptrarray" => {
                    let idx = index_of(&mut p.names.ptr_arrays, g(1)?.atom()?);
                    let info = g(3)?.atom()?;
                    let is_asset = g(5)?.flag()?;
                    let nonleaf = g(6)?.flag()?;
                    let f = PtrArrayFn {
                        size: g(2)?.num()?,
                        load: (info != "-" && nonleaf && !is_asset)
                            .then(|| index_of(&mut p.names.loads, info)),
                        reusable: g(4)?.flag()?,
                        asset: if is_asset {
                            Some(p.asset(g(3)?)?)
                        } else {
                            None
                        },
                        align: p.eval(g(7)?)?,
                    };
                    if ptr_arrays.len() <= idx {
                        ptr_arrays.resize_with(idx + 1, || None);
                    }
                    ptr_arrays[idx] = Some(f);
                }
                "loadptr" => {
                    root = Some((
                        index_of(&mut p.names.loads, g(1)?.atom()?),
                        g(2)?.flag()?,
                        p.eval(g(3)?)?,
                    ));
                }
                _ => return Err(bad),
            }
        }

        let at = asset_sx.at();
        let (root, root_in_temp, root_align) = root.ok_or(missing(at))?;
        pad(&mut loads, names.loads.len());
        pad(&mut arrays, names.arrays.len());
        pad(&mut ptr_arrays, names.ptr_arrays.len());
        assets.push(AssetSchema {
            name: asset_name.into(),
            loads: loads
                .into_iter()
                .collect::<Option<_>>()
                .ok_or(missing(at))?,
            arrays: arrays
                .into_iter()
                .collect::<Option<_>>()
                .ok_or(missing(at))?,
            ptr_arrays: ptr_arrays
                .into_iter()
                .collect::<Option<_>>()
                .ok_or(missing(at))?,
            root,
            root_in_temp,
            root_align,
            var_count: names.vars.len(),
        });
    }
    Ok(Schema { assets })
}
