use bevy_ecs::prelude::{Resource, World};
use std::collections::BTreeMap;

use crate::script::host::natives;
use crate::script::{Namespace, Runtime, Value};

pub type Native = fn(&mut World, &Value, &[Value]) -> Result<Value, String>;

#[derive(Resource, Clone)]
pub struct NativeRegistry(BTreeMap<(Namespace, String), Native>);
impl Default for NativeRegistry {
    fn default() -> Self {
        let mut registry = Self(BTreeMap::new());
        registry.register(Namespace::Function, "isdefined", |world, _, args| {
            if args.len() != 1 {
                return Err("isdefined expects one argument".into());
            }
            let defined = match &args[0] {
                Value::Undefined => false,
                Value::Object(id) => world.resource::<Runtime>().live(id),
                _ => true,
            };
            Ok(Value::Int(i32::from(defined)))
        });
        registry.register(Namespace::Function, "gettime", |world, _, args| {
            if !args.is_empty() {
                return Err("gettime expects no arguments".into());
            }
            let tick = world.resource::<crate::step::StepRequest>().tick;
            let ms = u64::from(tick.0) * u64::from(crate::MATCH_TICK_MS);
            i32::try_from(ms)
                .map(Value::Int)
                .map_err(|_| "gettime overflow".into())
        });
        registry.register(Namespace::Function, "spawnstruct", |world, _, args| {
            if !args.is_empty() {
                return Err("spawnstruct expects no arguments".into());
            }
            let mut runtime = world.resource_mut::<Runtime>();
            let id = runtime.next_object;
            runtime.next_object = id.checked_add(1).ok_or("object identifier exhausted")?;
            runtime.objects.insert(id, BTreeMap::new());
            Ok(Value::Object(id))
        });
        natives::iw4::register(&mut registry);
        super::audio::register(&mut registry);
        natives::math::register(&mut registry);
        natives::engine::register(&mut registry);
        natives::player::register(&mut registry);
        natives::skill::register(&mut registry);
        natives::local_profile::register(&mut registry);
        super::objectives::register(&mut registry);
        super::weapons::register(&mut registry);
        super::vehicles::register(&mut registry);
        super::physics::register(&mut registry);
        natives::t5::register(&mut registry);
        super::controls::register(&mut registry);
        super::client_effects::register(&mut registry);
        super::guidance::register(&mut registry);
        super::turrets::register(&mut registry);
        super::triggers::register(&mut registry);
        super::spectators::register(&mut registry);
        registry
    }
}
impl NativeRegistry {
    pub fn register(&mut self, namespace: Namespace, name: &str, native: Native) {
        self.0
            .insert((namespace, name.to_ascii_lowercase()), native);
    }
    pub(crate) fn get(&self, namespace: Namespace, name: &str) -> Option<Native> {
        self.0.get(&(namespace, name.to_owned())).copied()
    }
}

pub(crate) fn unavailable(
    world: &mut World,
    name: &'static str,
    reason: &str,
) -> Result<Value, String> {
    let mut runtime = world.resource_mut::<Runtime>();
    let calls = runtime.unsupported.entry(name).or_default();
    *calls = calls.saturating_add(1);
    Err(format!("unavailable: {reason}"))
}
