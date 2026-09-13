use std::net::SocketAddr;

use bevy::prelude::*;

use crate::authority::runtime::AuthorityWorld;
use crate::transport::protocol::{ContentFingerprint, HandshakeHello, MatchDescriptor};
use crate::transport::udp_session::{UdpAuthorityHub, UdpClientLink};

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub enum UdpLaunchIntent {
    None,

    Bind(SocketAddr),

    Connect(SocketAddr),
}

impl UdpLaunchIntent {
    pub fn from_env() -> Self {
        if let Ok(raw) = std::env::var("IW4L_UDP_BIND") {
            match raw.parse::<SocketAddr>() {
                Ok(addr) => return Self::Bind(addr),
                Err(e) => {
                    diag::warn!(Net, "IW4L_UDP_BIND={raw:?} parse failed: {e}");
                }
            }
        }
        if let Ok(raw) = std::env::var("IW4L_UDP_CONNECT") {
            match raw.parse::<SocketAddr>() {
                Ok(addr) => return Self::Connect(addr),
                Err(e) => {
                    diag::warn!(Net, "IW4L_UDP_CONNECT={raw:?} parse failed: {e}");
                }
            }
        }
        Self::None
    }
}

pub fn handshake_hello_for_udp(
    authority: Option<&AuthorityWorld>,
    descriptor: Option<&MatchDescriptor>,
) -> Option<HandshakeHello> {
    if std::env::var_os("IW4L_UDP_GAMEPLAY").is_some() {
        static WARN_GAMEPLAY_ENV: std::sync::Once = std::sync::Once::new();
        WARN_GAMEPLAY_ENV.call_once(|| {
            diag::warn!(
                Net,
                "IW4L_UDP_GAMEPLAY is ignored; admission uses MatchDescriptor (authored map/weapons/classes)"
            );
        });
    }
    let descriptor = descriptor?;
    let mut content = ContentFingerprint {
        map: descriptor.map,
        weapons: descriptor.weapons,
        classes: descriptor.classes,
        gameplay: 0,
        models: 0,
    };
    if let Some(world) = authority.filter(|world| world.0.has_world_clip()) {
        let live = ContentFingerprint::from_world(&world.0);
        content.gameplay = live.gameplay;
        content.models = live.models;
    }
    Some(HandshakeHello::current(content))
}

pub fn arm_udp_authority_hub(
    intent: Res<UdpLaunchIntent>,
    role: Res<crate::RuntimeRole>,
    existing: Option<Res<UdpAuthorityHub>>,
    authority: Res<AuthorityWorld>,
    descriptor: Option<Res<MatchDescriptor>>,
    mut commands: Commands,
) {
    if existing.is_some() {
        return;
    }
    let UdpLaunchIntent::Bind(addr) = *intent else {
        return;
    };
    let Some(descriptor) = descriptor.as_deref() else {
        return;
    };
    let Some(hello) = handshake_hello_for_udp(Some(&authority), Some(descriptor)) else {
        return;
    };

    let first = if *role == crate::RuntimeRole::Listen {
        1
    } else {
        0
    };
    match UdpAuthorityHub::bind_with_first_client(addr, hello, first) {
        Ok(hub) => {
            match hub.local_addr() {
                Ok(bound) => {
                    diag::info!(Net, "udp authority bound on {bound} first_client={first}")
                }
                Err(_) => diag::info!(Net, "udp authority bound on {addr} first_client={first}"),
            }
            commands.insert_resource(hub);
        }
        Err(e) => diag::warn!(Net, "udp bind {addr} failed: {e}"),
    }
}

pub fn refresh_udp_authority_hello(
    hub: Option<ResMut<UdpAuthorityHub>>,
    authority: Res<AuthorityWorld>,
    descriptor: Option<Res<MatchDescriptor>>,
) {
    let Some(mut hub) = hub else {
        return;
    };
    let Some(descriptor) = descriptor.as_deref() else {
        return;
    };
    let Some(hello) = handshake_hello_for_udp(Some(&authority), Some(descriptor)) else {
        return;
    };
    let content = hello.content;
    if hub.hello.content == content {
        return;
    }
    let was = hub.hello.content;

    if (was.map, was.weapons, was.classes) != (content.map, content.weapons, content.classes) {
        diag::info!(
            Net,
            "udp authority content now map={:016x} weapons={:016x} classes={:016x} (was {:016x} / {:016x} / {:016x})",
            content.map,
            content.weapons,
            content.classes,
            was.map,
            was.weapons,
            was.classes
        );
    }
    hub.hello.content = content;
}

pub fn arm_udp_client_link(
    intent: Res<UdpLaunchIntent>,
    role: Res<crate::RuntimeRole>,
    existing: Option<Res<UdpClientLink>>,
    authority: Option<Res<AuthorityWorld>>,
    descriptor: Option<Res<MatchDescriptor>>,
    mut commands: Commands,
) {
    if existing.is_some() {
        return;
    }
    if *role == crate::RuntimeRole::Listen {
        return;
    }
    let UdpLaunchIntent::Connect(addr) = *intent else {
        return;
    };
    let Some(descriptor) = descriptor.as_deref() else {
        return;
    };
    let Some(hello) = handshake_hello_for_udp(authority.as_deref(), Some(descriptor)) else {
        return;
    };
    match UdpClientLink::connect(addr, hello) {
        Ok(link) => {
            diag::info!(Net, "udp client targeting {addr}");
            commands.insert_resource(link);
        }
        Err(e) => diag::warn!(Net, "udp connect setup {addr} failed: {e}"),
    }
}

pub fn refresh_udp_client_hello(
    link: Option<ResMut<UdpClientLink>>,
    authority: Option<Res<AuthorityWorld>>,
    descriptor: Option<Res<MatchDescriptor>>,
) {
    let Some(mut link) = link else {
        return;
    };
    let Some(hello) = handshake_hello_for_udp(authority.as_deref(), descriptor.as_deref()) else {
        return;
    };
    if link.hello.content == hello.content {
        return;
    }
    let was = link.hello.content;
    if (was.map, was.weapons, was.classes)
        != (
            hello.content.map,
            hello.content.weapons,
            hello.content.classes,
        )
    {
        diag::info!(
            Net,
            "udp client content now map={:016x} weapons={:016x} classes={:016x} (was {:016x} / {:016x} / {:016x})",
            hello.content.map,
            hello.content.weapons,
            hello.content.classes,
            was.map,
            was.weapons,
            was.classes
        );
    }
    link.hello = hello;
    link.limits = hello.limits;
}

pub fn register_udp_launch(app: &mut App) {
    app.insert_resource(UdpLaunchIntent::from_env());
    if app
        .world()
        .resource::<crate::RuntimeRole>()
        .runs_authority()
    {
        app.add_systems(
            FixedUpdate,
            (arm_udp_authority_hub, refresh_udp_authority_hello)
                .chain()
                .in_set(crate::AuthoritySet::Advance),
        );
    }
    if app.world().resource::<crate::RuntimeRole>().runs_client() {
        app.add_systems(
            Update,
            (arm_udp_client_link, refresh_udp_client_hello)
                .chain()
                .in_set(crate::ClientSet::Load),
        );
    }
    app.add_systems(
        Update,
        crate::signon::drive_match_boundary
            .in_set(crate::ClientSet::Load)
            .after(frame::SessionSwapApplied),
    );
}
