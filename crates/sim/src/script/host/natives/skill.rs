use super::super::args::{arg, float, string};
use crate::script::{Namespace, NativeRegistry, Value};

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Function, "updateskill", |world, _, args| {
        if args.len() != 4 {
            return Err("updateskill needs two players, a mode and a score".into());
        }
        super::player::check_data_write(world)?;
        let first = crate::ClientId(super::player::player(world, arg(args, 0)?)?);
        let second = crate::ClientId(super::player::player(world, arg(args, 1)?)?);
        let mode = string(args, 2)?;
        let score = float(args, 3)?;
        world
            .resource_mut::<crate::PersistentDataStore>()
            .update_skill(first, second, &mode, score)
            .map_err(|error| format!("skill rating: {error:?}"))?;
        Ok(Value::Undefined)
    });
}
