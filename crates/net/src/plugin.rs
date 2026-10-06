use bevy::prelude::*;

use crate::authority::inbox::{AuthorityClock, ClientActionInbox, ClientCommandInbox};
use crate::client::presentation::presented::{LocalPresentClient, PresentedSnapshot};
use crate::schedule::{configure_authority_sets, configure_client_sets};
use crate::transport::loopback_live::ListenLoopback;
use frame::RuntimeRole;

pub struct NetPlugin {
    pub role: RuntimeRole,
}

impl NetPlugin {
    pub const fn listen() -> Self {
        Self {
            role: RuntimeRole::Listen,
        }
    }

    pub const fn dedicated() -> Self {
        Self {
            role: RuntimeRole::Dedicated,
        }
    }

    pub const fn client() -> Self {
        Self {
            role: RuntimeRole::Client,
        }
    }

    pub const fn replay() -> Self {
        Self {
            role: RuntimeRole::Replay,
        }
    }
}

impl Plugin for NetPlugin {
    fn build(&self, app: &mut App) {
        let master_enabled = app
            .world()
            .get_resource::<crate::MasterLaunchIntent>()
            .is_some_and(crate::MasterLaunchIntent::enabled);
        app.init_resource::<crate::AccountSaveReceipt>();
        app.insert_resource(self.role)
            .init_resource::<PresentedSnapshot>()
            .init_resource::<LocalPresentClient>()
            .init_resource::<crate::MasterMatchStart>()
            .init_resource::<crate::client::presentation::presented::ViewweaponAim>();
        app.add_systems(Last, crate::time_scale::apply_time_scale);
        {
            configure_authority_sets(app);
            app.init_resource::<AuthorityClock>()
                .init_resource::<ClientCommandInbox>()
                .init_resource::<ClientActionInbox>()
                .init_resource::<crate::ClientActionLedger>()
                .init_resource::<crate::ActionRequestIds>()
                .init_resource::<crate::ReliableEventHub>()
                .insert_resource(Time::<Fixed>::from_hz(
                    crate::authority::inbox::AUTHORITY_HZ,
                ));
        }
        if self.role.runs_client() {
            configure_client_sets(app);
            app.init_resource::<crate::client::input::LookState>()
                .init_resource::<crate::client::input::ClientActionInput>();

            app.init_resource::<ListenLoopback>();
            app.init_resource::<crate::authority::runtime::AuthorityInputGate>();
            if self.role == RuntimeRole::Client {
                app.init_resource::<crate::authority::runtime::AuthorityLoadHold>()
                    .init_resource::<crate::authority::inbox::AuthorityClock>();
            }
            if !self.role.runs_authority() {
                app.init_resource::<ClientCommandInbox>()
                    .init_resource::<ClientActionInbox>();
            }
            crate::client::runtime::register_client_runtime(app);
            crate::client::presentation::entities::register_client_entities(app);
            crate::client::presentation::entity_event_dispatch::register_entity_event_dispatch(app);
        }

        crate::authority::runtime::register_listen_runtime(app);
        if self.role == RuntimeRole::Listen
            || self.role == RuntimeRole::Client
            || self.role == RuntimeRole::Replay
        {
            crate::client::runtime::register_listen_prediction_arm(app);
        }
        if self.role == RuntimeRole::Replay {
            app.insert_resource(crate::authority::runtime::AuthorityLoadHold(true));
        }
        if master_enabled {
            crate::transport::master::register_master_bridge(app);
        } else {
            app.add_systems(
                Update,
                crate::signon::drive_match_boundary
                    .in_set(crate::ClientSet::Load)
                    .after(frame::SessionSwapApplied),
            );
        }
        crate::observe::register(app);
        app.insert_resource(crate::DeferHostWorldReady::from_env())
            .insert_resource(crate::DeferBootstrapApplied::from_env());
    }
}
