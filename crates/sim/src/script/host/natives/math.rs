use super::super::args::{arg, float, int, kind, optional, string, vector};
use super::super::arrays::{array_values, iteration_key, new_array};
use super::super::tables::{table, table_lookup, table_lookup_by_row, table_search};
use super::iw4::atoi;
use crate::script::{ArrayKey, Namespace, NativeRegistry, Runtime, Value};
use bevy_ecs::prelude::World;

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub(crate) fn distance_sq(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = sub(a, b);
    dot(d, d)
}
fn normalize(v: [f32; 3]) -> [f32; 3] {
    let length = dot(v, v).sqrt();
    if length == 0.0 {
        v
    } else {
        scale(v, 1.0 / length)
    }
}

fn nearest_on_segment(a: [f32; 3], b: [f32; 3], point: [f32; 3]) -> [f32; 3] {
    let segment = sub(b, a);
    let length_sq = dot(segment, segment);
    if length_sq == 0.0 {
        return a;
    }
    let t = (dot(sub(point, a), segment) / length_sq).clamp(0.0, 1.0);
    add(a, scale(segment, t))
}

pub(crate) fn random(world: &mut World) -> u32 {
    let mut runtime = world.resource_mut::<Runtime>();
    let mut x = runtime.rng;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    runtime.rng = x;
    x
}

fn random_unit(world: &mut World) -> f32 {
    (random(world) >> 8) as f32 / (1u32 << 24) as f32
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    use Namespace::Function;
    macro_rules! unary {
        ($($name:literal => $f:expr),* $(,)?) => {$(
            registry.register(Function, $name, |_, _, args| {
                let f: fn(f32) -> f32 = $f;
                Ok(Value::Float(f(float(args, 0)?)))
            });
        )*};
    }
    unary! {
        "abs" => f32::abs,
        "ceil" => f32::ceil,
        "floor" => f32::floor,
        "cos" => |d| d.to_radians().cos(),
        "sin" => |d| d.to_radians().sin(),
        "tan" => |d| d.to_radians().tan(),
        "angleclamp180" => |a| {
            let a = a - 360.0 * (a / 360.0).floor();
            if a > 180.0 { a - 360.0 } else { a }
        },
        "angleclamp" => |a| a - 360.0 * (a / 360.0).floor(),
    }
    macro_rules! inverse {
        ($($name:literal => $f:expr),* $(,)?) => {$(
            registry.register(Function, $name, |_, _, args| {
                let value = float(args, 0)?;
                if !(-1.0..=1.0).contains(&value) {
                    return Err(format!("{value} out of range"));
                }
                let f: fn(f32) -> f32 = $f;
                Ok(Value::Float(f(value).to_degrees()))
            });
        )*};
    }
    inverse! { "acos" => f32::acos, "asin" => f32::asin }
    registry.register(Function, "atan", |_, _, args| {
        Ok(Value::Float(float(args, 0)?.atan().to_degrees()))
    });
    registry.register(Function, "sqrt", |_, _, args| {
        let value = float(args, 0)?;
        if value < 0.0 {
            return Err("negative sqrt".into());
        }
        Ok(Value::Float(value.sqrt()))
    });
    registry.register(Function, "int", |_, _, args| {
        Ok(Value::Int(match arg(args, 0)? {
            Value::Int(n) => *n,
            Value::Float(n) => *n as i32,
            Value::String(s) => atoi(s),
            other => return Err(format!("cannot cast {} to int", kind(other))),
        }))
    });
    registry.register(Function, "max", |_, _, args| {
        Ok(Value::Float(float(args, 0)?.max(float(args, 1)?)))
    });
    registry.register(Function, "min", |_, _, args| {
        Ok(Value::Float(float(args, 0)?.min(float(args, 1)?)))
    });
    registry.register(Function, "distance", |_, _, args| {
        Ok(Value::Float(
            distance_sq(vector(args, 0)?, vector(args, 1)?).sqrt(),
        ))
    });
    registry.register(Function, "distancesquared", |_, _, args| {
        Ok(Value::Float(distance_sq(
            vector(args, 0)?,
            vector(args, 1)?,
        )))
    });
    registry.register(Function, "distance2d", |_, _, args| {
        let (a, b) = (vector(args, 0)?, vector(args, 1)?);
        Ok(Value::Float(
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt(),
        ))
    });
    registry.register(Function, "length", |_, _, args| {
        let v = vector(args, 0)?;
        Ok(Value::Float(dot(v, v).sqrt()))
    });
    registry.register(Function, "lengthsquared", |_, _, args| {
        let v = vector(args, 0)?;
        Ok(Value::Float(dot(v, v)))
    });
    registry.register(Function, "vectordot", |_, _, args| {
        Ok(Value::Float(dot(vector(args, 0)?, vector(args, 1)?)))
    });
    registry.register(Function, "vectornormalize", |_, _, args| {
        Ok(Value::Vector(normalize(vector(args, 0)?)))
    });
    registry.register(Function, "vectortoangles", |_, _, args| {
        Ok(Value::Vector(math_iw4::vect_to_angles(vector(args, 0)?)))
    });
    registry.register(Function, "vectortoyaw", |_, _, args| {
        let v = vector(args, 0)?;
        Ok(Value::Float(math_iw4::vec_to_yaw(v[0], v[1])))
    });
    macro_rules! axis {
        ($($name:literal => $pick:expr),* $(,)?) => {$(
            registry.register(Function, $name, |_, _, args| {
                let axes = math_iw4::angle_vectors(vector(args, 0)?);
                let pick: fn(([f32; 3], [f32; 3], [f32; 3])) -> [f32; 3] = $pick;
                Ok(Value::Vector(pick(axes)))
            });
        )*};
    }
    axis! {
        "anglestoforward" => |a| a.0,
        "anglestoright" => |a| a.1,
        "anglestoup" => |a| a.2,
    }
    registry.register(Function, "pointonsegmentnearesttopoint", |_, _, args| {
        Ok(Value::Vector(nearest_on_segment(
            vector(args, 0)?,
            vector(args, 1)?,
            vector(args, 2)?,
        )))
    });
    registry.register(Function, "vectorfromlinetopoint", |_, _, args| {
        let (a, b, point) = (vector(args, 0)?, vector(args, 1)?, vector(args, 2)?);
        let direction = normalize(sub(b, a));
        let along = dot(sub(point, a), direction);
        Ok(Value::Vector(sub(point, add(a, scale(direction, along)))))
    });
    registry.register(Function, "averagepoint", |world, _, args| {
        let points = array_values(world, arg(args, 0)?)?;
        let mut sum = [0.0; 3];
        for point in &points {
            let Value::Vector(v) = point else {
                return Err("averagepoint expects an array of vectors".into());
            };
            sum = add(sum, *v);
        }
        Ok(Value::Vector(if points.is_empty() {
            sum
        } else {
            scale(sum, 1.0 / points.len() as f32)
        }))
    });
    registry.register(Function, "averagenormal", |world, _, args| {
        let normals = array_values(world, arg(args, 0)?)?;
        let mut sum = [0.0; 3];
        for normal in &normals {
            let Value::Vector(v) = normal else {
                return Err("averagenormal expects an array of vectors".into());
            };
            sum = add(sum, *v);
        }
        Ok(Value::Vector(normalize(sum)))
    });
    registry.register(Function, "randomint", |world, _, args| {
        let max = int(args, 0)?;
        if max <= 0 {
            return Ok(Value::Int(0));
        }
        Ok(Value::Int((random(world) % max as u32) as i32))
    });
    registry.register(Function, "randomintrange", |world, _, args| {
        let (min, max) = (int(args, 0)?, int(args, 1)?);
        if max <= min {
            return Ok(Value::Int(min));
        }
        let span = (i64::from(max) - i64::from(min)) as u32;
        Ok(Value::Int(min.wrapping_add((random(world) % span) as i32)))
    });
    registry.register(Function, "randomfloat", |world, _, args| {
        let max = float(args, 0)?;
        Ok(Value::Float(random_unit(world) * max))
    });
    registry.register(Function, "randomfloatrange", |world, _, args| {
        let (min, max) = (float(args, 0)?, float(args, 1)?);
        Ok(Value::Float(min + random_unit(world) * (max - min)))
    });
    registry.register(Function, "getsubstr", |_, _, args| {
        let text = super::super::args::byte_string(args, 0)?;
        let bytes = text.as_bytes();
        let start = int(args, 1)?.max(0) as usize;
        let end = match optional(args, 2, int)? {
            Some(end) => (end.max(0) as usize).min(bytes.len()),
            None => bytes.len(),
        };
        Ok(Value::byte_string(
            bytes.get(start.min(end)..end).unwrap_or(&[]),
        ))
    });
    registry.register(Function, "issubstr", |_, _, args| {
        let text = super::super::args::byte_string(args, 0)?;
        let needle = super::super::args::byte_string(args, 1)?;
        Ok(Value::Int(i32::from(
            needle.is_empty()
                || text
                    .as_bytes()
                    .windows(needle.len())
                    .any(|window| window == needle.as_bytes()),
        )))
    });
    registry.register(Function, "strtok", |world, _, args| {
        let text = super::super::args::byte_string(args, 0)?;
        let delimiters = super::super::args::byte_string(args, 1)?;
        let tokens = text
            .as_bytes()
            .split(|byte| delimiters.as_bytes().contains(byte))
            .filter(|token| !token.is_empty())
            .map(Value::byte_string)
            .collect();
        new_array(world, tokens)
    });
    registry.register(Function, "getfirstarraykey", |world, _, args| {
        if args.len() != 1 {
            return Err("GetFirstArrayKey expects an array".into());
        }
        iteration_key(world, arg(args, 0)?, None)
    });
    registry.register(Function, "getnextarraykey", |world, _, args| {
        if args.len() != 2 {
            return Err("GetNextArrayKey expects an array and previous key".into());
        }
        iteration_key(world, arg(args, 0)?, Some(arg(args, 1)?))
    });
    registry.register(Function, "getarraykeys", |world, _, args| {
        let Value::Array(id) = arg(args, 0)? else {
            return Err("getarraykeys expects an array".into());
        };
        let keys: Vec<_> = world
            .resource::<Runtime>()
            .arrays
            .get(id)
            .ok_or("invalid array reference")?
            .keys()
            .rev()
            .map(|key| match key {
                ArrayKey::Integer(i) => Value::Int(*i),
                ArrayKey::String(s) => Value::String(s.clone()),
            })
            .collect();
        new_array(world, keys)
    });
    registry.register(Function, "tablelookup", |world, _, args| {
        Ok(Value::String(table_lookup(world, args)?.into()))
    });
    registry.register(Function, "tablelookupistring", |world, _, args| {
        Ok(Value::LocalizedString(table_lookup(world, args)?.into()))
    });
    registry.register(Function, "tablelookupbyrow", |world, _, args| {
        Ok(Value::String(table_lookup_by_row(world, args)?.into()))
    });
    registry.register(Function, "tablelookupistringbyrow", |world, _, args| {
        Ok(Value::LocalizedString(table_lookup_by_row(world, args)?))
    });
    registry.register(Function, "tablelookuprownum", |world, _, args| {
        let tables = world.resource::<Runtime>().tables.clone();
        let name = string(args, 0)?;
        let column = int(args, 1)?;
        let value = string(args, 2)?;
        let row = table(&tables, &name)
            .filter(|_| column >= 0)
            .and_then(|t| table_search(t, column as usize, &value));
        Ok(Value::Int(row.map_or(-1, |r| r as i32)))
    });
    registry.register(Function, "getsystemtime", |_, _, _| {
        let seconds = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| "system time precedes the Unix epoch")?
            .as_secs();
        let seconds =
            u32::try_from(seconds).map_err(|_| "system time exceeds the timestamp format")?;
        Ok(Value::Int(i32::from_le_bytes(seconds.to_le_bytes())))
    });
    registry.register(Function, "getbuildnumber", |_, _, _| {
        env!("IW4L_BUILD_NUMBER")
            .parse::<i32>()
            .map(Value::Int)
            .map_err(|_| "numeric build metadata is unavailable".into())
    });
    registry.register(Function, "getbuildversion", |_, _, _| {
        Ok(Value::string(env!("CARGO_PKG_VERSION")))
    });
}
