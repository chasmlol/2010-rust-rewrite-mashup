use bevy::prelude::Entity;
use entity_iw4::{EntityEventAction, EntityEventKind, entity_event_action};

pub(crate) fn recipient(event: EntityEventKind, live: Option<Entity>) -> Option<Entity> {
    if matches!(
        entity_event_action(event),
        Ok(EntityEventAction::Explosion | EntityEventAction::PhysicsSphere)
    ) {
        return Some(Entity::PLACEHOLDER);
    }
    live.or_else(|| without_centity(event).then_some(Entity::PLACEHOLDER))
}

fn without_centity(event: EntityEventKind) -> bool {
    matches!(
        entity_event_action(event),
        Ok(EntityEventAction::PlayFx | EntityEventAction::Obituary | EntityEventAction::Rumble)
    ) || event == EntityEventKind::PLAY_RUMBLE_ON_POS
        || event == EntityEventKind::STOPSOUNDS
        || event == EntityEventKind::SOUND_ALIAS
        || event == EntityEventKind::SOUND_ALIAS_AS_MASTER
}
