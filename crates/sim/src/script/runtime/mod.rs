mod lifecycle;
mod state;

use super::{
    ArrayKey, Binary, Callee, Fault, Frame, Global, Location, Op, Program, Thread, ThreadState,
    Unary, Value, VecDeque, Waiter, WaiterKind,
};
use bevy_ecs::entity::Entity;
use bevy_ecs::prelude::World;
use std::collections::BTreeMap;
use std::sync::Arc;

pub(crate) use lifecycle::{copy_state, install, install_level, reset};
pub(crate) use state::Runtime;

/// Instructions one resumed thread may run before it is killed as a runaway loop.
/// Level startup runs in one frame and needs over a million on the larger maps.
pub(super) const INSTRUCTION_BUDGET: usize = 16 * 1_000_000;

pub(crate) fn take_signals(world: &mut World) -> Vec<Arc<str>> {
    std::mem::take(&mut world.resource_mut::<Runtime>().signals)
}

pub(super) fn level_endon_armed(world: &World, name: &str) -> bool {
    world.resource::<Runtime>().waiters.iter().any(|w| {
        w.receiver == Value::level()
            && &*w.name == name
            && matches!(w.kind, WaiterKind::Endon { .. })
    })
}

pub(super) fn return_from(world: &mut World, function: &str, now: i64) -> usize {
    let Some(&function) = world
        .resource::<Runtime>()
        .program
        .as_ref()
        .and_then(|program| program.names.get(function))
    else {
        return 0;
    };
    let inside: Vec<_> = world
        .query::<(Entity, &Thread)>()
        .iter(world)
        .filter_map(|(entity, thread)| {
            let depth = thread.frames.iter().position(|f| f.function == function)?;
            Some((entity, depth))
        })
        .collect();
    for &(entity, depth) in &inside {
        let mut thread = world.entity_mut(entity).take::<Thread>().unwrap();
        unwind(world, &mut thread, depth, now, false);
        world.entity_mut(entity).insert(thread);
    }
    inside.len()
}

pub(super) fn raise(world: &mut World, receiver: Value, name: &str, args: Vec<Value>) {
    world
        .resource_mut::<Runtime>()
        .pending_notifies
        .push((receiver, name.into(), args));
}

fn deliver_pending(world: &mut World, thread: &mut Thread, now: i64) -> Result<(), String> {
    loop {
        let pending = std::mem::take(&mut world.resource_mut::<Runtime>().pending_notifies);
        if pending.is_empty() {
            return Ok(());
        }
        for (receiver, name, args) in pending {
            notify(world, thread, &receiver, &name, &args, now)?;
        }
    }
}

fn deliver_external(world: &mut World, now: i64) {
    let mut carrier = Thread {
        serial: u64::MAX,
        frames: Vec::new(),
        stack: Vec::new(),
        state: ThreadState::Complete,
    };
    let pending = std::mem::take(&mut world.resource_mut::<Runtime>().pending_notifies);
    let program = world.resource::<Runtime>().program.clone().unwrap();
    for (receiver, name, args) in pending {
        let result = notify(world, &mut carrier, &receiver, &name, &args, now);
        if let Err(message) = result {
            world.resource_mut::<Runtime>().fault = Some(Fault::at(
                &Location {
                    module: "<engine>".into(),
                    function: "notify".into(),
                    line: 0,
                    column: 0,
                },
                message,
            ));
            break;
        }
        // A touch handler can wait again before the next toucher is notified.
        run_ready(world, &program, now);
        if world.resource::<Runtime>().fault.is_some() {
            break;
        }
    }
}

fn frame(
    program: &Program,
    function: usize,
    args: Vec<Value>,
    stack_base: usize,
    receiver: Value,
) -> Frame {
    let definition = &program.functions[function];
    let mut locals = vec![Value::Undefined; definition.slots];
    for (slot, value) in locals.iter_mut().zip(args).take(definition.parameters) {
        *slot = value;
    }
    Frame {
        function,
        pc: 0,
        stack_base,
        receiver,
        locals,
    }
}

fn entry(world: &World, name: &str) -> Result<(Arc<Program>, usize, Location), Fault> {
    let location = Location {
        module: "<runtime>".into(),
        function: name.into(),
        line: 0,
        column: 0,
    };
    let runtime = world.resource::<Runtime>();
    if let Some(fault) = &runtime.fault {
        return Err(fault.clone());
    }
    let program = runtime
        .program
        .as_ref()
        .ok_or_else(|| Fault::at(&location, "no loaded GSC program"))?
        .clone();
    let function = *program
        .names
        .get(&name.replace('\\', "/").to_ascii_lowercase())
        .ok_or_else(|| Fault::at(&location, "unknown script entry point"))?;
    Ok((program, function, location))
}

pub(crate) fn start(
    world: &mut World,
    name: &str,
    receiver: Value,
    args: Vec<Value>,
) -> Result<u64, Fault> {
    let (program, function, location) = entry(world, name)?;
    let mut runtime = world.resource_mut::<Runtime>();
    if runtime.last_tick.is_none()
        && receiver == Value::level()
        && args.is_empty()
        && let Some(plan) = runtime.restart.as_mut()
    {
        Arc::make_mut(plan).entries.push(name.into());
    }
    spawn_thread(world, &program, function, receiver, args).map_err(|m| Fault::at(&location, m))
}

pub(super) fn run_now(
    world: &mut World,
    name: &str,
    receiver: Value,
    args: Vec<Value>,
    now: i64,
) -> Result<(), Fault> {
    let (program, function, location) = entry(world, name)?;
    let mut thread = new_thread(world, &program, function, receiver, args)
        .map_err(|m| Fault::at(&location, m))?;
    thread.state = ThreadState::Runnable;
    let budget = std::mem::replace(
        &mut world.resource_mut::<Runtime>().budget,
        INSTRUCTION_BUDGET,
    );
    execute(world, &program, &mut thread, now);
    world.resource_mut::<Runtime>().budget = budget;
    if thread.state == ThreadState::Complete {
        retire(&mut world.resource_mut::<Runtime>(), thread.serial);
    } else {
        world.spawn(thread);
    }
    match world.resource::<Runtime>().fault.clone() {
        Some(fault) => Err(fault),
        None => Ok(()),
    }
}

fn spawn_thread(
    world: &mut World,
    program: &Program,
    function: usize,
    receiver: Value,
    args: Vec<Value>,
) -> Result<u64, String> {
    let thread = new_thread(world, program, function, receiver, args)?;
    let serial = thread.serial;
    world.resource_mut::<Runtime>().spawned.push(serial);
    world.spawn(thread);
    Ok(serial)
}

pub(super) fn new_thread(
    world: &mut World,
    program: &Program,
    function: usize,
    receiver: Value,
    args: Vec<Value>,
) -> Result<Thread, String> {
    let args = args
        .into_iter()
        .map(|value| copy_value(world, value))
        .collect::<Result<Vec<_>, _>>()?;
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.started = true;
    let serial = runtime.next_serial;
    runtime.next_serial = serial.checked_add(1).ok_or("thread identifier exhausted")?;
    Ok(Thread {
        serial,
        frames: vec![frame(program, function, args, 0, receiver)],
        stack: Vec::new(),
        state: ThreadState::Queued,
    })
}

const MAX_FRAMES: usize = 31;

fn frame_room(world: &World, thread: &Thread) -> Result<(), String> {
    if world.resource::<Runtime>().suspended_frames + thread.frames.len() >= MAX_FRAMES {
        return Err("script stack overflow (too many embedded function calls)".into());
    }
    Ok(())
}

fn settle_owed_deaths(world: &mut World, parent: &mut Thread, now: i64) -> Result<(), String> {
    {
        let runtime = world.resource::<Runtime>();
        if runtime.deaths.is_empty() || runtime.current_hit.is_some() {
            return Ok(());
        }
    }
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.suspended.push(parent.serial);
    runtime.suspended_frames += parent.frames.len();
    super::host::players::settle_deaths(world);
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.suspended.pop();
    runtime.suspended_frames -= parent.frames.len();
    resume_suspended(world, parent, now)
}

fn resume_suspended(world: &mut World, parent: &mut Thread, now: i64) -> Result<(), String> {
    let mut runtime = world.resource_mut::<Runtime>();
    if runtime.fault.is_some() {
        return Err(String::new());
    }
    let depth = runtime
        .pending_unwinds
        .iter()
        .filter(|(serial, _)| *serial == parent.serial)
        .map(|(_, depth)| *depth)
        .min();
    runtime
        .pending_unwinds
        .retain(|(serial, _)| *serial != parent.serial);
    if let Some(depth) = depth.filter(|depth| *depth < parent.frames.len()) {
        unwind(world, parent, depth, now, true);
    }
    Ok(())
}

fn run_inline(
    world: &mut World,
    program: &Program,
    parent: &mut Thread,
    mut child: Thread,
    now: i64,
) -> Result<(), String> {
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.suspended.push(parent.serial);
    runtime.suspended_frames += parent.frames.len();
    child.state = ThreadState::Runnable;
    execute(world, program, &mut child, now);
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.suspended.pop();
    runtime.suspended_frames -= parent.frames.len();
    if child.state == ThreadState::Complete {
        retire(&mut runtime, child.serial);
    } else {
        world.spawn(child);
    }
    parent.stack.push(Value::Undefined);
    resume_suspended(world, parent, now)
}

fn pop(thread: &mut Thread) -> Result<Value, String> {
    thread
        .stack
        .pop()
        .ok_or_else(|| "invalid IR: stack underflow".into())
}

pub(super) fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Undefined => "undefined",
        Value::Int(_) => "int",
        Value::Float(_) => "float",
        Value::String(_) => "string",
        Value::LocalizedString(_) => "localized string",
        Value::Vector(_) => "vector",
        Value::Object(_) => "object",
        Value::Array(_) => "array",
        Value::Function(_) => "function",
        Value::Builtin(_) => "builtin function",
        Value::Animation { .. } => "animation",
        Value::AnimationTree(_) => "animtree",
    }
}

fn mismatch(a: &Value, b: &Value) -> String {
    format!(
        "pair '{}' and '{}' has unmatching types",
        type_name(a),
        type_name(b)
    )
}

fn truth(value: &Value) -> Result<bool, String> {
    match value {
        Value::Int(n) => Ok(*n != 0),
        Value::Float(n) => Ok(*n != 0.0),
        _ => Err(format!("cast from {} to bool", type_name(value))),
    }
}

fn scalar(value: &Value) -> Option<f32> {
    match value {
        Value::Int(n) => Some(*n as f32),
        Value::Float(n) => Some(*n),
        _ => None,
    }
}

fn weaker(a: Value, b: Value) -> (Value, Value) {
    match (&a, &b) {
        (Value::Int(x), Value::Float(_)) => (Value::Float(*x as f32), b),
        (Value::Float(_), Value::Int(y)) => (a, Value::Float(*y as f32)),
        (Value::Vector(_), _) if scalar(&b).is_some() => {
            let y = scalar(&b).unwrap();
            (a, Value::Vector([y; 3]))
        }
        (_, Value::Vector(_)) if scalar(&a).is_some() => {
            let x = scalar(&a).unwrap();
            (Value::Vector([x; 3]), b)
        }
        _ => (a, b),
    }
}

fn format_g(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let trim = |text: &str| -> String {
        if text.contains('.') {
            text.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            text.to_owned()
        }
    };
    let scientific = format!("{value:.5e}");
    let (mantissa, exponent) = scientific.split_once('e').unwrap();
    let exponent: i32 = exponent.parse().unwrap();
    if !(-4..6).contains(&exponent) {
        let sign = if exponent < 0 { '-' } else { '+' };
        format!("{}e{sign}{:03}", trim(mantissa), exponent.abs())
    } else {
        trim(&format!("{value:.*}", (5 - exponent) as usize))
    }
}

pub(super) fn to_text(value: &Value) -> Option<String> {
    match value {
        Value::Int(n) => Some(n.to_string()),
        // format_g needs the exponent that NaN and infinity print without.
        Value::Float(n) if n.is_finite() => Some(format_g(f64::from(*n))),
        Value::Vector(v) if v.iter().all(|n| n.is_finite()) => Some(format!(
            "({}, {}, {})",
            format_g(f64::from(v[0])),
            format_g(f64::from(v[1])),
            format_g(f64::from(v[2]))
        )),
        _ => None,
    }
}

fn equality(a: Value, b: Value) -> Result<bool, String> {
    let (a, b) = weaker(a, b);
    Ok(match (&a, &b) {
        (Value::Array(_), _) | (_, Value::Array(_)) => {
            return Err("cannot compare arrays".into());
        }
        (Value::Undefined, Value::Undefined) => true,
        (Value::Object(_), Value::Object(_)) => a == b,
        (Value::String(x), Value::String(y)) => x == y,
        (Value::LocalizedString(x), Value::LocalizedString(y)) => x == y,
        (Value::Vector(x), Value::Vector(y)) => x == y,
        (Value::Float(x), Value::Float(y)) => (x - y).abs() < 1e-6,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Function(x), Value::Function(y)) => x == y,
        (Value::Builtin(x), Value::Builtin(y)) => x == y,
        (Value::Animation { .. }, Value::Animation { .. })
        | (Value::AnimationTree(_), Value::AnimationTree(_)) => a == b,
        _ => return Err(mismatch(&a, &b)),
    })
}

pub(super) fn unary(op: Unary, value: Value) -> Result<Value, String> {
    match (op, &value) {
        (Unary::Not, _) => Ok(Value::Int(i32::from(!truth(&value)?))),
        (Unary::Complement, Value::Int(n)) => Ok(Value::Int(!n)),
        (Unary::Complement, _) => Err(format!("~ cannot be applied to {}", type_name(&value))),
    }
}

pub(super) fn binary(op: Binary, a: Value, b: Value) -> Result<Value, String> {
    let integers = |a: &Value, b: &Value| match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok((*x, *y)),
        _ => Err(format!(
            "{op:?} requires int operands, found {} and {}",
            type_name(a),
            type_name(b)
        )),
    };
    let result = match op {
        Binary::Or => integers(&a, &b).map(|(x, y)| Value::Int(x | y))?,
        Binary::Xor => integers(&a, &b).map(|(x, y)| Value::Int(x ^ y))?,
        Binary::And => integers(&a, &b).map(|(x, y)| Value::Int(x & y))?,
        Binary::Shl => {
            integers(&a, &b).map(|(x, y)| Value::Int(x.wrapping_shl((y & 31) as u32)))?
        }
        Binary::Shr => {
            integers(&a, &b).map(|(x, y)| Value::Int(x.wrapping_shr((y & 31) as u32)))?
        }
        Binary::Mod => {
            let (x, y) = integers(&a, &b)?;
            Value::Int(x.checked_rem(y).unwrap_or(0))
        }
        Binary::Equal => Value::Int(i32::from(equality(a, b)?)),
        Binary::NotEqual => Value::Int(i32::from(!equality(a, b)?)),
        Binary::Case => match (&a, &b) {
            (Value::Int(_) | Value::String(_), _) => Value::Int(i32::from(a == b)),
            _ => return Err(format!("cannot switch on {}", type_name(&a))),
        },
        Binary::Add => {
            let (a, b) = match (&a, &b) {
                (Value::String(_), _) | (_, Value::String(_)) => {
                    let bytes = |value: &Value| match value {
                        Value::String(text) => Some(text.as_bytes().to_vec()),
                        other => to_text(other).map(String::into_bytes),
                    };
                    match (bytes(&a), bytes(&b)) {
                        (Some(mut x), Some(y)) => {
                            if x.len() + y.len() > 0x2000 {
                                return Err("string too long".into());
                            }
                            x.extend_from_slice(&y);
                            return Ok(Value::byte_string(&x));
                        }
                        _ => return Err(mismatch(&a, &b)),
                    }
                }
                (Value::Int(x), Value::Float(_)) => (Value::Float(*x as f32), b),
                (Value::Float(_), Value::Int(y)) => (a, Value::Float(*y as f32)),
                _ => (a, b),
            };
            match (&a, &b) {
                (Value::Int(x), Value::Int(y)) => Value::Int(x.wrapping_add(*y)),
                (Value::Float(x), Value::Float(y)) => Value::Float(x + y),
                (Value::Vector(x), Value::Vector(y)) => {
                    Value::Vector([x[0] + y[0], x[1] + y[1], x[2] + y[2]])
                }
                _ => return Err(mismatch(&a, &b)),
            }
        }
        Binary::Sub | Binary::Mul | Binary::Div => {
            let (a, b) = weaker(a, b);
            match (&a, &b, op) {
                (Value::Int(x), Value::Int(y), Binary::Sub) => Value::Int(x.wrapping_sub(*y)),
                (Value::Int(x), Value::Int(y), Binary::Mul) => Value::Int(x.wrapping_mul(*y)),
                // The stock scripts divide by a zero they never guard (a caps-per-minute
                // rate read before the first second is counted) and read the zero back.
                (Value::Int(_), Value::Int(0), _) | (_, Value::Float(0.0), Binary::Div) => {
                    Value::Float(0.0)
                }
                (Value::Int(x), Value::Int(y), _) => Value::Float(*x as f32 / *y as f32),
                (Value::Float(x), Value::Float(y), Binary::Sub) => Value::Float(x - y),
                (Value::Float(x), Value::Float(y), Binary::Mul) => Value::Float(x * y),
                (Value::Float(x), Value::Float(y), _) => Value::Float(x / y),
                (Value::Vector(x), Value::Vector(y), Binary::Sub) => {
                    Value::Vector([x[0] - y[0], x[1] - y[1], x[2] - y[2]])
                }
                (Value::Vector(x), Value::Vector(y), Binary::Mul) => {
                    Value::Vector([x[0] * y[0], x[1] * y[1], x[2] * y[2]])
                }
                (Value::Vector(_), Value::Vector(y), _) if y.contains(&0.0) => {
                    Value::Vector([0.0; 3])
                }
                (Value::Vector(x), Value::Vector(y), _) => {
                    Value::Vector([x[0] / y[0], x[1] / y[1], x[2] / y[2]])
                }
                _ => return Err(mismatch(&a, &b)),
            }
        }
        Binary::Less | Binary::Greater | Binary::LessEqual | Binary::GreaterEqual => {
            let (a, b) = weaker(a, b);
            let ordered = match (&a, &b, op) {
                (Value::Int(x), Value::Int(y), Binary::Less | Binary::GreaterEqual) => x < y,
                (Value::Int(x), Value::Int(y), _) => x > y,
                (Value::Float(x), Value::Float(y), Binary::Less | Binary::GreaterEqual) => x < y,
                (Value::Float(x), Value::Float(y), _) => x > y,
                _ => return Err(mismatch(&a, &b)),
            };
            let inverted = matches!(op, Binary::GreaterEqual | Binary::LessEqual);
            Value::Int(i32::from(ordered != inverted))
        }
    };
    match &result {
        Value::Float(n) if !n.is_finite() => Err("non-finite arithmetic result".into()),
        Value::Vector(v) if !v.iter().all(|n| n.is_finite()) => {
            Err("non-finite arithmetic result".into())
        }
        _ => Ok(result),
    }
}

fn object_key(runtime: &mut Runtime, key: ArrayKey) -> Result<u32, String> {
    match key {
        ArrayKey::String(key) => Ok(runtime.symbol(&key.symbol_key())),
        ArrayKey::Integer(_) => Err("object index must be a string".into()),
    }
}

fn instruction(
    world: &mut World,
    program: &Program,
    thread: &mut Thread,
    op: Op,
    now: i64,
) -> Result<(), String> {
    let spawn = matches!(op, Op::Spawn(..) | Op::Indirect(_, _, true));
    let method = matches!(
        op,
        Op::Call(_, _, true) | Op::Spawn(_, _, true) | Op::Indirect(_, true, _)
    );
    match op {
        Op::Constant(value) => thread.stack.push(value),
        Op::FunctionRef(_) => return Err("invalid IR: unlinked function reference".into()),
        Op::Global(global) => thread.stack.push(match global {
            Global::SelfRef => thread.frames.last().unwrap().receiver.clone(),
            Global::Level => Value::Object(0),
            Global::Game => Value::Object(1),
            Global::Anim => Value::Object(2),
        }),
        Op::Load(slot) => {
            let value = thread.frames.last().unwrap().locals[slot as usize].clone();
            thread.stack.push(value);
        }
        Op::Store(slot) => {
            let value = copy_value(world, pop(thread)?)?;
            thread.frames.last_mut().unwrap().locals[slot as usize] = value;
        }
        Op::Dup => {
            let value = thread
                .stack
                .last()
                .cloned()
                .ok_or("invalid IR: stack underflow")?;
            thread.stack.push(value);
        }
        Op::DupPair => {
            let base = thread
                .stack
                .len()
                .checked_sub(2)
                .ok_or("invalid IR: stack underflow")?;
            let pair = thread.stack[base..].to_vec();
            thread.stack.extend(pair);
        }
        Op::ArrayKeys => {
            let Value::Array(id) = pop(thread)? else {
                return Err("foreach requires an array".into());
            };
            let keys: Vec<_> = world
                .resource::<Runtime>()
                .arrays
                .get(&id)
                .ok_or("invalid array reference")?
                .keys()
                .map(|key| match key {
                    ArrayKey::Integer(i) => Value::Int(*i),
                    ArrayKey::String(s) => Value::String(s.clone()),
                })
                .collect();
            let value = allocate_array(world)?;
            let Value::Array(id) = value else {
                unreachable!()
            };
            let mut runtime = world.resource_mut::<Runtime>();
            let array = runtime.arrays.get_mut(&id).unwrap();
            for (i, key) in keys.into_iter().enumerate() {
                array.insert(ArrayKey::Integer(i as i32), key);
            }
            thread.stack.push(value);
        }
        Op::EnsureLocalArray(slot) => {
            if thread.frames.last().unwrap().locals[slot as usize] == Value::Undefined {
                let value = allocate_array(world)?;
                thread.frames.last_mut().unwrap().locals[slot as usize] = value;
            }
        }
        Op::EnsureFieldArray(field) => {
            let receiver = pop(thread)?;
            let Value::Object(id) = receiver else {
                return Err("array field requires an object".into());
            };
            let value = world
                .resource::<Runtime>()
                .objects
                .get(&id)
                .ok_or("invalid object reference")?
                .get(&field)
                .cloned()
                .unwrap_or(Value::Undefined);
            let value = if value == Value::Undefined {
                let value = allocate_array(world)?;
                world
                    .resource_mut::<Runtime>()
                    .objects
                    .get_mut(&id)
                    .unwrap()
                    .insert(field, value.clone());
                value
            } else {
                value
            };
            thread.stack.push(value);
        }
        Op::EnsureIndexArray => {
            let key = array_key(pop(thread)?)?;
            let receiver = pop(thread)?;
            let mut runtime = world.resource_mut::<Runtime>();
            let (value, field) = match &receiver {
                Value::Array(id) => (
                    runtime
                        .arrays
                        .get(id)
                        .ok_or("invalid array reference")?
                        .get(&key)
                        .cloned(),
                    None,
                ),
                Value::Object(id) => {
                    let field = object_key(&mut runtime, key.clone())?;
                    (
                        runtime
                            .objects
                            .get(id)
                            .ok_or("invalid object reference")?
                            .get(&field)
                            .cloned(),
                        Some(field),
                    )
                }
                _ => return Err("index assignment requires array or object".into()),
            };
            let value = match value {
                Some(value) if value != Value::Undefined => value,
                _ => {
                    let value = allocate_array(world)?;
                    let mut runtime = world.resource_mut::<Runtime>();
                    match receiver {
                        Value::Array(id) => {
                            runtime
                                .arrays
                                .get_mut(&id)
                                .unwrap()
                                .insert(key, value.clone());
                        }
                        Value::Object(id) => {
                            runtime
                                .objects
                                .get_mut(&id)
                                .unwrap()
                                .insert(field.unwrap(), value.clone());
                        }
                        _ => unreachable!(),
                    }
                    value
                }
            };
            thread.stack.push(value);
        }
        Op::Array => {
            thread.stack.push(allocate_array(world)?);
        }
        Op::LoadIndex => {
            let key = array_key(pop(thread)?)?;
            let receiver = pop(thread)?;
            let mut runtime = world.resource_mut::<Runtime>();
            let value = match receiver {
                Value::Array(id) => runtime
                    .arrays
                    .get(&id)
                    .ok_or("invalid array reference")?
                    .get(&key)
                    .cloned()
                    .unwrap_or(Value::Undefined),
                Value::Object(id) => {
                    let field = object_key(&mut runtime, key)?;
                    runtime
                        .objects
                        .get(&id)
                        .ok_or("invalid object reference")?
                        .get(&field)
                        .cloned()
                        .unwrap_or(Value::Undefined)
                }
                Value::Vector(v) => {
                    let ArrayKey::Integer(i) = key else {
                        return Err("vector index must be an integer".into());
                    };
                    Value::Float(*v.get(i as usize).ok_or("vector index out of range")?)
                }
                Value::String(s) => {
                    let ArrayKey::Integer(i) = key else {
                        return Err("string index must be an integer".into());
                    };
                    s.as_bytes()
                        .get(i as usize)
                        .map(|byte| Value::byte_string(&[*byte]))
                        .unwrap_or(Value::Undefined)
                }
                _ => return Err("value cannot be indexed".into()),
            };
            thread.stack.push(value);
        }
        Op::StoreIndex => {
            let value = copy_value(world, pop(thread)?)?;
            let key = array_key(pop(thread)?)?;
            let receiver = pop(thread)?;
            let mut runtime = world.resource_mut::<Runtime>();
            match receiver {
                Value::Array(id) => {
                    let values = runtime
                        .arrays
                        .get_mut(&id)
                        .ok_or("invalid array reference")?;
                    if value == Value::Undefined {
                        values.remove(&key);
                    } else {
                        values.insert(key, value);
                    }
                }
                Value::Object(id) => {
                    let field = object_key(&mut runtime, key)?;
                    let fields = runtime
                        .objects
                        .get_mut(&id)
                        .ok_or("invalid object reference")?;
                    if value == Value::Undefined {
                        fields.remove(&field);
                    } else {
                        fields.insert(field, value);
                    }
                }
                _ => return Err("index assignment requires array or object".into()),
            }
        }
        Op::Pop => {
            pop(thread)?;
        }
        Op::Unary(op) => {
            let value = pop(thread)?;
            thread.stack.push(unary(op, value)?);
        }
        Op::Binary(op) => {
            let b = pop(thread)?;
            let a = pop(thread)?;
            thread.stack.push(binary(op, a, b)?);
        }
        Op::Vector => {
            let mut v = [0.0; 3];
            for component in v.iter_mut().rev() {
                let value = pop(thread)?;
                *component = scalar(&value)
                    .ok_or_else(|| format!("vector component cannot be {}", type_name(&value)))?;
            }
            thread.stack.push(Value::Vector(v));
        }
        Op::Jump(pc) => thread.frames.last_mut().unwrap().pc = pc,
        Op::JumpFalse(pc) => {
            if !truth(&pop(thread)?)? {
                thread.frames.last_mut().unwrap().pc = pc;
            }
        }
        Op::Call(_, argc, _) | Op::Spawn(_, argc, _) | Op::Indirect(argc, _, _) => {
            let base = thread
                .stack
                .len()
                .checked_sub(argc)
                .ok_or("invalid IR: argument underflow")?;
            let args = thread
                .stack
                .split_off(base)
                .into_iter()
                .map(|value| copy_value(world, value))
                .collect::<Result<Vec<_>, _>>()?;
            let callee = match op {
                Op::Call(callee, _, _) | Op::Spawn(callee, _, _) => callee,
                Op::Indirect(..) => match pop(thread)? {
                    Value::Function(id) => Callee::Script(id),
                    Value::Builtin(id) => Callee::Native(id),
                    other => {
                        return Err(format!(
                            "indirect call requires a function reference, found {}",
                            type_name(&other)
                        ));
                    }
                },
                _ => unreachable!(),
            };
            let receiver = if method {
                pop(thread)?
            } else {
                thread.frames.last().unwrap().receiver.clone()
            };
            match callee {
                Callee::Script(function) if spawn => {
                    frame_room(world, thread)?;
                    let child = new_thread(world, program, function as usize, receiver, args)?;
                    run_inline(world, program, thread, child, now)?;
                }
                Callee::Script(function) => {
                    frame_room(world, thread)?;
                    thread.frames.push(frame(
                        program,
                        function as usize,
                        args,
                        thread.stack.len(),
                        receiver,
                    ));
                }
                Callee::Native(_) if spawn => {
                    return Err("threading native calls is unsupported".into());
                }
                Callee::Native(index) => {
                    let native = world.resource::<Runtime>().natives[index as usize];
                    let name = &program.natives[index as usize].name;
                    // A defect in one builtin costs its caller an undefined result, not the match.
                    let value = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        native(world, &receiver, &args)
                    }))
                    .unwrap_or_else(|_| Err("builtin panicked".into()))
                    .map_err(|m| format!("{name}: {m}"))?;
                    thread.stack.push(value);
                    deliver_pending(world, thread, now)?;
                    settle_owed_deaths(world, thread, now)?;
                }
                Callee::Unlinked(_) => return Err("invalid IR: unlinked call".into()),
            }
        }
        Op::FrameEnd => {
            thread.state = ThreadState::Queued;
            world
                .resource_mut::<Runtime>()
                .buckets
                .entry(now)
                .or_default()
                .push_back(thread.serial);
        }
        Op::Wait => {
            let value = pop(thread)?;
            let frames = match value {
                Value::Int(n) => n.wrapping_mul(20),
                Value::Float(n) if n < 0.0 => return Err("negative wait is not allowed".into()),
                Value::Float(n) => match (n * 20.0 + 0.5).floor() {
                    0.0 if n != 0.0 => 1,
                    f if f < 2_147_483_648.0 => f as i32,
                    _ => i32::MIN,
                },
                other => return Err(format!("type {} is not a float", type_name(&other))),
            };
            if frames as u32 >= 0xff_ffff {
                return Err(if frames < 0 {
                    "negative wait is not allowed".into()
                } else {
                    "wait is too long".into()
                });
            }
            let until = now + i64::from(frames) * i64::from(crate::MATCH_TICK_MS);
            thread.state = ThreadState::Queued;
            world
                .resource_mut::<Runtime>()
                .buckets
                .entry(until)
                .or_default()
                .push_front(thread.serial);
        }
        Op::Size => {
            let receiver = pop(thread)?;
            let size = match &receiver {
                Value::Array(id) => world
                    .resource::<Runtime>()
                    .arrays
                    .get(id)
                    .ok_or("invalid array reference")?
                    .len(),
                Value::Object(_) => 1,
                Value::String(s) => s.len(),
                other => return Err(format!("size cannot be applied to {}", type_name(other))),
            };
            thread.stack.push(Value::Int(size as i32));
        }
        Op::LoadField(field) => {
            let receiver = pop(thread)?;
            let Value::Object(id) = receiver else {
                return Err(format!(
                    "field receiver must be an object or entity, found {}",
                    type_name(&receiver)
                ));
            };
            if let Some(client) = world.resource::<Runtime>().player_client(id)
                && let Some(value) = super::host::players::load_field(
                    world,
                    client,
                    &program.symbols[field as usize],
                )
            {
                thread.stack.push(value);
                return Ok(());
            }
            if let Some(value) = super::host::mechanics::load_slide_field(
                world,
                id,
                &program.symbols[field as usize],
            ) {
                thread.stack.push(value);
                return Ok(());
            }
            let runtime = world.resource::<Runtime>();
            let fields = runtime
                .objects
                .get(&id)
                .ok_or("invalid script object reference")?;
            thread
                .stack
                .push(fields.get(&field).cloned().unwrap_or(Value::Undefined));
        }
        Op::StoreField(field) => {
            let value = copy_value(world, pop(thread)?)?;
            let receiver = pop(thread)?;
            let Value::Object(id) = receiver else {
                return Err("native entity fields are not bound".into());
            };
            if let Some(client) = world.resource::<Runtime>().player_client(id)
                && super::host::players::store_field(
                    world,
                    client,
                    &program.symbols[field as usize],
                    &value,
                )?
            {
                return Ok(());
            }
            if super::host::mechanics::store_slide_field(
                world,
                id,
                &program.symbols[field as usize],
                &value,
            )? {
                return Ok(());
            }
            super::host::hud::store_field(world, id, &program.symbols[field as usize], &value)?;
            let mut runtime = world.resource_mut::<Runtime>();
            let fields = runtime
                .objects
                .get_mut(&id)
                .ok_or("invalid script object reference")?;
            if value == Value::Undefined {
                fields.remove(&field);
            } else {
                fields.insert(field, value);
            }
        }
        Op::Notify(argc) => {
            let base = thread
                .stack
                .len()
                .checked_sub(argc)
                .ok_or("invalid IR: event argument underflow")?;
            let arguments = thread.stack.split_off(base);
            let name = event_name(pop(thread)?)?;
            let receiver = event_receiver(pop(thread)?)?;
            notify(world, thread, &receiver, &name, &arguments, now)?;
        }
        Op::Await(outputs) => {
            let name = event_name(pop(thread)?)?;
            let receiver = event_receiver(pop(thread)?)?;
            register(
                world,
                thread,
                receiver,
                name,
                WaiterKind::Waittill { outputs },
            );
        }
        Op::AwaitMatch(argc) => {
            let base = thread
                .stack
                .len()
                .checked_sub(argc)
                .ok_or("invalid IR: event argument underflow")?;
            let values = thread.stack.split_off(base);
            let name = event_name(pop(thread)?)?;
            let receiver = event_receiver(pop(thread)?)?;
            register(world, thread, receiver, name, WaiterKind::Match { values });
        }
        Op::Endon => {
            let name = event_name(pop(thread)?)?;
            let receiver = event_receiver(pop(thread)?)?;
            let frame = thread.frames.len() - 1;
            world.resource_mut::<Runtime>().waiters.push(Waiter {
                receiver,
                name,
                thread: thread.serial,
                kind: WaiterKind::Endon { frame },
            });
        }
        Op::Return => {
            let value = pop(thread)?;
            let depth = thread.frames.len() - 1;
            let serial = thread.serial;
            world.resource_mut::<Runtime>().waiters.retain(|w| {
                w.thread != serial
                    || !matches!(w.kind, WaiterKind::Endon { frame } if frame >= depth)
            });
            let frame = thread
                .frames
                .pop()
                .ok_or("invalid IR: return without frame")?;
            thread.stack.truncate(frame.stack_base);
            if thread.frames.is_empty() {
                thread.state = ThreadState::Complete;
            } else {
                thread.stack.push(value);
            }
        }
    }
    Ok(())
}

fn event_name(value: Value) -> Result<Arc<str>, String> {
    match value {
        Value::String(name) => Ok(name.symbol_key()),
        other => Err(format!(
            "event name must be a string, found {}",
            type_name(&other)
        )),
    }
}
fn event_receiver(value: Value) -> Result<Value, String> {
    match value {
        Value::Object(_) => Ok(value),
        other => Err(format!("{} is not an object", type_name(&other))),
    }
}

fn register(
    world: &mut World,
    thread: &mut Thread,
    receiver: Value,
    name: Arc<str>,
    kind: WaiterKind,
) {
    thread.state = ThreadState::Awaiting;
    world.resource_mut::<Runtime>().waiters.push(Waiter {
        receiver,
        name,
        thread: thread.serial,
        kind,
    });
}

fn find_thread(world: &mut World, serial: u64) -> Option<Entity> {
    world
        .query::<(Entity, &Thread)>()
        .iter(world)
        .find(|(_, t)| t.serial == serial)
        .map(|(e, _)| e)
}

fn with_thread<R>(
    world: &mut World,
    current: &mut Thread,
    serial: u64,
    f: impl FnOnce(&mut World, &mut Thread, bool) -> R,
) -> Option<R> {
    if serial == current.serial {
        return Some(f(world, current, true));
    }
    let entity = find_thread(world, serial)?;
    let mut thread = world.entity_mut(entity).take::<Thread>().unwrap();
    let result = f(world, &mut thread, false);
    if thread.state == ThreadState::Complete {
        world.despawn(entity);
    } else {
        world.entity_mut(entity).insert(thread);
    }
    Some(result)
}

fn retire(runtime: &mut Runtime, serial: u64) {
    runtime.waiters.retain(|w| w.thread != serial);
    dequeue(runtime, serial);
}

fn dequeue(runtime: &mut Runtime, serial: u64) {
    for bucket in runtime.buckets.values_mut() {
        bucket.retain(|s| *s != serial);
    }
    runtime.spawned.retain(|s| *s != serial);
}

fn resume_now(world: &mut World, thread: &mut Thread, now: i64) {
    thread.state = ThreadState::Queued;
    world
        .resource_mut::<Runtime>()
        .buckets
        .entry(now)
        .or_default()
        .push_front(thread.serial);
}

fn unwind(world: &mut World, thread: &mut Thread, depth: usize, now: i64, running: bool) {
    let serial = thread.serial;
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.waiters.retain(|w| {
        w.thread != serial || matches!(w.kind, WaiterKind::Endon { frame } if frame < depth)
    });
    if !running {
        dequeue(&mut runtime, serial);
    }
    let base = thread.frames[depth].stack_base;
    thread.frames.truncate(depth);
    thread.stack.truncate(base);
    if depth == 0 {
        thread.state = ThreadState::Complete;
        return;
    }
    thread.stack.push(Value::Undefined);
    if running {
        thread.state = ThreadState::Runnable;
    } else {
        resume_now(world, thread, now);
    }
}

fn payload_matches(values: &[Value], arguments: &[Value]) -> bool {
    values.len() <= arguments.len()
        && values
            .iter()
            .zip(arguments)
            .all(|(v, a)| equality(v.clone(), a.clone()) == Ok(true))
}

fn notify(
    world: &mut World,
    current: &mut Thread,
    receiver: &Value,
    name: &Arc<str>,
    arguments: &[Value],
    now: i64,
) -> Result<(), String> {
    if *receiver == Value::Object(0) {
        world.resource_mut::<Runtime>().signals.push(name.clone());
    }
    loop {
        let runtime = world.resource::<Runtime>();
        let Some(index) = runtime.waiters.iter().position(|w| {
            &w.receiver == receiver
                && &w.name == name
                && match &w.kind {
                    WaiterKind::Match { values } => payload_matches(values, arguments),
                    _ => true,
                }
        }) else {
            return Ok(());
        };
        let waiter = world.resource_mut::<Runtime>().waiters.remove(index);
        let result = match waiter.kind {
            WaiterKind::Endon { frame } => {
                let mut runtime = world.resource_mut::<Runtime>();
                if runtime.suspended.contains(&waiter.thread) {
                    runtime.pending_unwinds.push((waiter.thread, frame));
                    runtime.waiters.retain(|w| {
                        w.thread != waiter.thread
                            || matches!(w.kind, WaiterKind::Endon { frame: f } if f < frame)
                    });
                    continue;
                }
                with_thread(world, current, waiter.thread, |world, thread, running| {
                    unwind(world, thread, frame, now, running);
                    Ok::<_, String>(())
                })
            }
            WaiterKind::Waittill { outputs } => {
                with_thread(world, current, waiter.thread, |world, thread, _| {
                    for (i, slot) in outputs.iter().enumerate() {
                        let value = match arguments.get(i) {
                            Some(value) => copy_value(world, value.clone())?,
                            None => Value::Undefined,
                        };
                        thread.frames.last_mut().unwrap().locals[*slot as usize] = value;
                    }
                    resume_now(world, thread, now);
                    Ok(())
                })
            }
            WaiterKind::Match { .. } => {
                with_thread(world, current, waiter.thread, |world, thread, _| {
                    resume_now(world, thread, now);
                    Ok(())
                })
            }
        };
        result.unwrap_or(Ok(()))?;
    }
}

fn kill(world: &mut World, entity: Entity, serial: u64) {
    world.despawn(entity);
    retire(&mut world.resource_mut::<Runtime>(), serial);
}

pub(crate) fn advance_scheduler(world: &mut World) {
    let request = world.resource::<crate::step::StepRequest>();
    if !request.reason.advances_authority_world() {
        return;
    }
    let tick = request.tick;
    let runtime = world.resource::<Runtime>();
    if runtime.fault.is_some() {
        return;
    }
    let Some(program) = runtime.program.clone() else {
        return;
    };
    if runtime
        .last_tick
        .is_some_and(|previous| previous.0 >= tick.0)
    {
        world.resource_mut::<Runtime>().fault = Some(Fault::at(
            &Location {
                module: "<scheduler>".into(),
                function: String::new(),
                line: 0,
                column: 0,
            },
            "authority script ticks must increase; restore the runtime before replaying",
        ));
        return;
    }
    world.resource_mut::<Runtime>().last_tick = Some(tick);
    let now = i64::from(tick.0) * i64::from(crate::MATCH_TICK_MS);
    super::host::mechanics::deliver_finished(world);
    deliver_external(world, now);
    let threads: Vec<_> = world
        .query::<(Entity, &Thread)>()
        .iter(world)
        .map(|(entity, thread)| (entity, thread.serial, entity_receivers(world, thread)))
        .collect();
    let dead: Vec<_> = threads
        .into_iter()
        .filter(|(_, _, receivers)| any_deleted(world, receivers))
        .map(|(entity, serial, _)| (entity, serial))
        .collect();
    for (entity, serial) in dead {
        kill(world, entity, serial);
    }
    {
        let mut runtime = world.resource_mut::<Runtime>();
        let due: Vec<_> = runtime.buckets.range(..=now).map(|(k, _)| *k).collect();
        let mut current = VecDeque::new();
        for key in due {
            current.extend(runtime.buckets.remove(&key).unwrap());
        }
        for serial in std::mem::take(&mut runtime.spawned).into_iter().rev() {
            current.push_front(serial);
        }
        runtime.buckets.insert(now, current);
    }
    run_ready(world, &program, now);
    let deletes = std::mem::take(&mut world.resource_mut::<Runtime>().pending_deletes);
    for object in deletes {
        world.resource_mut::<Runtime>().delete_entity(object);
    }
    let mut runtime = world.resource_mut::<Runtime>();
    for id in std::mem::take(&mut runtime.dying) {
        if let Some(fields) = runtime.objects.get_mut(&id) {
            fields.clear();
        }
    }
    if runtime.buckets.get(&now).is_some_and(VecDeque::is_empty) {
        runtime.buckets.remove(&now);
    }
    runtime.loading = false;
    collect_heap(world);
}

fn run_ready(world: &mut World, program: &Program, now: i64) {
    loop {
        let next = world
            .resource_mut::<Runtime>()
            .buckets
            .get_mut(&now)
            .and_then(VecDeque::pop_front);
        let Some(serial) = next else {
            break;
        };
        let Some(entity) = find_thread(world, serial) else {
            continue;
        };
        let receivers = entity_receivers(world, world.get::<Thread>(entity).unwrap());
        if any_deleted(world, &receivers) {
            kill(world, entity, serial);
            continue;
        }
        let mut thread = world.entity_mut(entity).take::<Thread>().unwrap();
        thread.state = ThreadState::Runnable;
        world.resource_mut::<Runtime>().budget = INSTRUCTION_BUDGET;
        execute(world, program, &mut thread, now);
        if thread.state == ThreadState::Complete {
            kill(world, entity, serial);
        } else {
            world.entity_mut(entity).insert(thread);
        }
        if world.resource::<Runtime>().fault.is_some() {
            break;
        }
    }
}

pub(super) fn execute(world: &mut World, program: &Program, thread: &mut Thread, now: i64) {
    while thread.state == ThreadState::Runnable {
        let frame = thread.frames.last().unwrap();
        let function = &program.functions[frame.function];
        let Some((location, op)) = function.code.get(frame.pc).cloned() else {
            world.resource_mut::<Runtime>().fault = Some(Fault::at(
                &function.location,
                "invalid IR: instruction position out of range",
            ));
            break;
        };
        let exhausted = world.resource::<Runtime>().budget == 0;
        let (pops, pushes) = stack_effect(&op);
        let iterates = matches!(op, Op::ArrayKeys);
        let before = thread.stack.len();
        let result = if exhausted {
            Err("potential infinite loop in script - killing thread".into())
        } else {
            world.resource_mut::<Runtime>().budget -= 1;
            thread.frames.last_mut().unwrap().pc += 1;
            instruction(world, program, thread, op, now)
        };
        let Err(message) = result else {
            continue;
        };
        if world.resource::<Runtime>().fault.is_some() {
            break;
        }
        let mut fault = Fault::at(&location, message);
        fault.callers = thread
            .frames
            .iter()
            .rev()
            .skip(1)
            .rev()
            .map(|f| {
                program.functions[f.function].code[f.pc.saturating_sub(1)]
                    .0
                    .clone()
            })
            .collect();
        if terminal(&fault.message) {
            world.resource_mut::<Runtime>().fault = Some(fault);
            break;
        }
        report(world, &fault);
        if exhausted {
            let serial = thread.serial;
            let mut runtime = world.resource_mut::<Runtime>();
            runtime.budget = INSTRUCTION_BUDGET;
            runtime.waiters.retain(|w| w.thread != serial);
            thread.frames.clear();
            thread.stack.clear();
            thread.state = ThreadState::Complete;
            break;
        }
        // The failed operation leaves undefined in place of
        // its results and the thread carries on.
        let base = thread.frames.last().map_or(0, |f| f.stack_base);
        let Some(kept) = before.checked_sub(pops).filter(|kept| *kept >= base) else {
            world.resource_mut::<Runtime>().fault =
                Some(Fault::at(&location, "invalid IR: stack underflow"));
            break;
        };
        thread.stack.truncate(kept);
        thread.stack.resize(kept + pushes, Value::Undefined);
        // foreach walks keys from its operand and skips the body
        // when that is not an array; an undefined key list would never end.
        if iterates {
            match allocate_array(world) {
                Ok(empty) => *thread.stack.last_mut().unwrap() = empty,
                Err(message) => {
                    world.resource_mut::<Runtime>().fault = Some(Fault::at(&location, message));
                    break;
                }
            }
        }
    }
}

fn stack_effect(op: &Op) -> (usize, usize) {
    match *op {
        Op::Constant(_) | Op::FunctionRef(_) | Op::Global(_) | Op::Load(_) | Op::Array => (0, 1),
        Op::Dup => (0, 1),
        Op::DupPair => (0, 2),
        Op::Store(_) | Op::Pop | Op::JumpFalse(_) | Op::Wait => (1, 0),
        Op::Unary(_) | Op::ArrayKeys | Op::EnsureFieldArray(_) | Op::Size | Op::LoadField(_) => {
            (1, 1)
        }
        Op::Binary(_) | Op::EnsureIndexArray | Op::LoadIndex => (2, 1),
        Op::StoreField(_) | Op::Await(_) | Op::Endon => (2, 0),
        Op::StoreIndex => (3, 0),
        Op::Vector => (3, 1),
        Op::Call(_, argc, method) | Op::Spawn(_, argc, method) => (argc + usize::from(method), 1),
        Op::Indirect(argc, method, _) => (argc + 1 + usize::from(method), 1),
        Op::Notify(argc) | Op::AwaitMatch(argc) => (argc + 2, 0),
        Op::Jump(_) | Op::EnsureLocalArray(_) | Op::FrameEnd | Op::Return => (0, 0),
    }
}

fn terminal(message: &str) -> bool {
    message.starts_with("invalid IR")
        || message.contains("identifier exhausted")
        || message == "vehicle pool exhausted"
        || message.ends_with(": vehicle pool exhausted")
}

fn report(world: &mut World, fault: &Fault) {
    let site = format!(
        "{}:{}:{}",
        fault.location.module, fault.location.line, fault.location.column
    );
    let mut runtime = world.resource_mut::<Runtime>();
    let hits = runtime
        .errors
        .entry((site.clone(), fault.message.clone()))
        .or_default();
    *hits += 1;
    if *hits == 1 {
        diag::warn!(Sim, "gsc: script runtime error: {fault}");
        diag::script_boundary(
            "runtime_error",
            &format!(
                " at={site} function={} fault=\"{}\"",
                fault.location.function,
                fault.message.replace('"', "'")
            ),
        );
    }
}

pub(crate) fn healthy(runtime: bevy_ecs::prelude::Res<Runtime>) -> bool {
    runtime.fault.is_none()
}

pub(crate) fn preflight(
    world: &World,
    tick: crate::Tick,
    reason: crate::StepReason,
) -> Result<(), Fault> {
    let runtime = world.resource::<Runtime>();
    if let Some(fault) = &runtime.fault {
        return Err(fault.clone());
    }
    if reason.advances_authority_world() && runtime.last_tick.is_some_and(|last| last.0 >= tick.0) {
        return Err(Fault::at(
            &Location {
                module: "<scheduler>".into(),
                function: String::new(),
                line: 0,
                column: 0,
            },
            "authority script ticks must increase; restore the runtime before replaying",
        ));
    }
    if reason.advances_authority_world() && (!runtime.started || runtime.program.is_none()) {
        return Err(Fault::at(
            &Location {
                module: "<runtime>".into(),
                function: "step".into(),
                line: 0,
                column: 0,
            },
            "no loaded and started GSC program",
        ));
    }
    Ok(())
}

fn array_key(value: Value) -> Result<ArrayKey, String> {
    match value {
        Value::Int(n) => Ok(ArrayKey::Integer(n)),
        Value::String(s) => Ok(ArrayKey::String(s)),
        other => Err(format!(
            "array index must be an int or string, found {}",
            type_name(&other)
        )),
    }
}

fn allocate_array(world: &mut World) -> Result<Value, String> {
    let mut runtime = world.resource_mut::<Runtime>();
    let id = runtime.next_object;
    runtime.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
    runtime.arrays.insert(id, BTreeMap::new());
    Ok(Value::Array(id))
}

fn copy_value(world: &mut World, value: Value) -> Result<Value, String> {
    fn copy(
        world: &mut World,
        value: Value,
        depth: usize,
        remaining: &mut usize,
    ) -> Result<Value, String> {
        let Value::Array(id) = value else {
            return Ok(value);
        };
        if depth >= 256 || *remaining == 0 {
            return Err("array copy budget exceeded".into());
        }
        *remaining -= 1;
        let entries = world
            .resource::<Runtime>()
            .arrays
            .get(&id)
            .ok_or("invalid array reference")?
            .clone();
        if entries.len() > *remaining {
            return Err("array copy budget exceeded".into());
        }
        *remaining -= entries.len();
        let result = allocate_array(world)?;
        let Value::Array(new_id) = result else {
            unreachable!()
        };
        for (key, value) in entries {
            let value = copy(world, value, depth + 1, remaining)?;
            world
                .resource_mut::<Runtime>()
                .arrays
                .get_mut(&new_id)
                .unwrap()
                .insert(key, value);
        }
        Ok(result)
    }
    copy(world, value, 0, &mut 100_000)
}

/// Deleting a waited-on object ends the thread; a thread whose self is deleted keeps running.
fn entity_receivers(world: &World, thread: &Thread) -> Vec<Value> {
    world
        .resource::<Runtime>()
        .waiters
        .iter()
        .filter(|w| w.thread == thread.serial)
        .map(|w| &w.receiver)
        .filter(|value| matches!(value, Value::Object(_)))
        .cloned()
        .collect()
}

fn any_deleted(world: &World, receivers: &[Value]) -> bool {
    let runtime = world.resource::<Runtime>();
    receivers.iter().any(|value| {
        matches!(value, Value::Object(id) if runtime.dead.contains(id) || !runtime.objects.contains_key(id))
    })
}

impl Runtime {
    /// Values held outside script objects by the engine side: they keep what
    /// they reference alive exactly like a script variable would.
    fn native_roots(&self, pending: &mut Vec<Value>) {
        pending.extend(self.presented.values().flatten().cloned());
        for (receiver, _, args) in &self.pending_notifies {
            pending.push(receiver.clone());
            pending.extend(args.iter().cloned());
        }
        pending.extend(self.engine.world.map(Value::Object));
        pending.extend(
            self.engine
                .objectives
                .values()
                .filter_map(|o| o.entity())
                .cloned(),
        );
        for slot in self.players.values() {
            pending.push(Value::Object(slot.object));
            pending.extend(slot.presented.values().flatten().cloned());
        }
        for answers in self.menu_answers.values() {
            pending.extend(answers.iter().flat_map(|a| a.values()).cloned());
        }
        pending.extend(
            self.deaths
                .iter()
                .flat_map(|(_, _, args)| args.iter().cloned()),
        );
        self.t5.roots(pending);
    }
}

fn collect_heap(world: &mut World) {
    let mut pending = vec![Value::Object(0), Value::Object(1), Value::Object(2)];
    pending.extend(
        world
            .resource::<Runtime>()
            .entities
            .keys()
            .map(|id| Value::Object(*id)),
    );
    for thread in world.query::<&Thread>().iter(world) {
        pending.extend(thread.stack.iter().cloned());
        for frame in &thread.frames {
            pending.push(frame.receiver.clone());
            pending.extend(frame.locals.iter().cloned());
        }
    }
    let mut objects = std::collections::BTreeSet::new();
    let mut arrays = std::collections::BTreeSet::new();
    let mut runtime = world.resource_mut::<Runtime>();
    runtime.native_roots(&mut pending);
    for waiter in &runtime.waiters {
        pending.push(waiter.receiver.clone());
        if let WaiterKind::Match { values } = &waiter.kind {
            pending.extend(values.iter().cloned());
        }
    }
    while let Some(value) = pending.pop() {
        match value {
            Value::Object(id) if objects.insert(id) => {
                if let Some(fields) = runtime.objects.get(&id) {
                    pending.extend(fields.values().cloned());
                }
            }
            Value::Array(id) if arrays.insert(id) => {
                if let Some(values) = runtime.arrays.get(&id) {
                    pending.extend(values.values().cloned());
                }
            }
            _ => {}
        }
    }
    runtime.objects.retain(|id, _| objects.contains(id));
    runtime.dead.retain(|id| objects.contains(id));
    runtime.arrays.retain(|id, _| arrays.contains(id));
}
