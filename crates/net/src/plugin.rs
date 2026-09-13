use bevy::prelude::*;

use crate::authority::inbox::{AuthorityClock, ClientActionInbox, ClientCommandInbox};
use crate::client::presented::{LocalPresentClient, PresentedSnapshot};
use crate::role::RuntimeRole;
use crate::schedule::{configure_authority_sets, configure_client_sets};
use crate::transport::loopback_live::ListenLoopback;

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
        app.insert_resource(self.role)
            .init_resource::<PresentedSnapshot>()
            .init_resource::<LocalPresentClient>()
            .init_resource::<crate::MasterMatchStart>()
            .init_resource::<crate::client::presented::CgViewweaponAim>();
        if self.role.runs_authority() || self.role == RuntimeRole::Replay {
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
                app.init_resource::<crate::authority::runtime::AuthorityWorld>()
                    .init_resource::<crate::authority::runtime::AuthorityLoadHold>()
                    .init_resource::<crate::authority::inbox::AuthorityClock>();
            }
            if !self.role.runs_authority() {
                app.init_resource::<ClientCommandInbox>()
                    .init_resource::<ClientActionInbox>();
            }
            crate::client::runtime::register_client_runtime(app);
            crate::client::entities::register_client_entities(app);
            crate::client::entity_event_dispatch::register_entity_event_dispatch(app);
        }

        if self.role == RuntimeRole::Listen
            || self.role == RuntimeRole::Dedicated
            || self.role == RuntimeRole::Replay
        {
            crate::authority::runtime::register_listen_runtime(app);
        }
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
            crate::transport::udp_launch::register_udp_launch(app);
        }
        crate::observe::register(app);
        app.insert_resource(crate::DeferHostWorldReady::from_env())
            .insert_resource(crate::DeferBootstrapApplied::from_env());
    }
}
