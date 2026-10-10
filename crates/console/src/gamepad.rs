use bevy::input::gamepad::{Gamepad, GamepadAxis, GamepadButton, GamepadConnection, GamepadEvent};
use bevy::prelude::*;
use frame::{UiMenuKey, UiMenuRequest};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Sticks {
    /// Forward and right.
    pub movement: Vec2,
    /// Right and up.
    pub look: Vec2,
}

fn radial(stick: Vec2, deadzone: f32) -> Vec2 {
    let length = stick.length();
    if length <= deadzone || length <= f32::EPSILON {
        return Vec2::ZERO;
    }
    let scaled = ((length - deadzone) / (1.0 - deadzone).max(0.01)).min(1.0);
    stick / length * scaled
}

pub(crate) fn sticks(pad: &Gamepad, settings: &frame::GameSettings) -> Sticks {
    let left = radial(pad.left_stick(), settings.pad_deadzone_left);
    let right = radial(pad.right_stick(), settings.pad_deadzone_right);
    match settings.pad_stick_layout {
        1 => Sticks {
            movement: Vec2::new(right.y, right.x),
            look: left,
        },
        2 => Sticks {
            movement: Vec2::new(left.y, right.x),
            look: Vec2::new(left.x, right.y),
        },
        3 => Sticks {
            movement: Vec2::new(right.y, left.x),
            look: Vec2::new(right.x, left.y),
        },
        _ => Sticks {
            movement: Vec2::new(left.y, left.x),
            look: right,
        },
    }
}

fn curve(deflection: f32, kind: u8) -> f32 {
    let d = deflection.clamp(0.0, 1.0);
    match kind {
        1 => d,
        2 => 1.0 - (1.0 - d) * (1.0 - d),
        _ => 0.35 * d + 0.65 * d * d * d,
    }
}

pub(crate) fn shaped_look(look: Vec2, settings: &frame::GameSettings) -> Vec2 {
    let deflection = look.length();
    if deflection <= f32::EPSILON {
        return Vec2::ZERO;
    }
    let shaped = look / deflection * curve(deflection, settings.pad_curve);
    Vec2::new(
        shaped.x,
        if settings.pad_invert {
            -shaped.y
        } else {
            shaped.y
        },
    )
}

#[derive(Default)]
pub(crate) struct PadActivity {
    axes: std::collections::HashMap<(Entity, GamepadAxis), f32>,
    buttons: std::collections::HashMap<(Entity, GamepadButton), f32>,
}

pub(crate) fn track_active_pad(
    gamepads: Query<(Entity, &Gamepad, Option<&Name>)>,
    mut events: MessageReader<GamepadEvent>,
    mut active: ResMut<frame::ActivePad>,
    mut devices: ResMut<frame::InputDevices>,
    settings: Res<frame::GameSettings>,
    windows: Query<&Window, With<bevy::window::PrimaryWindow>>,
    mut activity: Local<PadActivity>,
) {
    devices.focused = windows.single().map_or(true, |window| window.focused);
    if active.0.is_some_and(|entity| gamepads.get(entity).is_err()) {
        active.0 = None;
    }
    let PadActivity { axes, buttons } = &mut *activity;
    axes.retain(|(entity, _), _| gamepads.contains(*entity));
    buttons.retain(|(entity, _), _| gamepads.contains(*entity));
    let mut connected = std::collections::HashSet::new();
    for event in events.read() {
        let activity = match event {
            GamepadEvent::Connection(event) => {
                if matches!(event.connection, GamepadConnection::Connected { .. }) {
                    connected.insert(event.gamepad);
                }
                if matches!(event.connection, GamepadConnection::Disconnected) {
                    axes.retain(|(entity, _), _| *entity != event.gamepad);
                    buttons.retain(|(entity, _), _| *entity != event.gamepad);
                    if active.0 == Some(event.gamepad) {
                        active.0 = None;
                    }
                }
                if matches!(event.connection, GamepadConnection::Connected { .. })
                    && let Ok((_, pad, _)) = gamepads.get(event.gamepad)
                {
                    for axis in [
                        GamepadAxis::LeftStickX,
                        GamepadAxis::LeftStickY,
                        GamepadAxis::RightStickX,
                        GamepadAxis::RightStickY,
                    ] {
                        axes.insert((event.gamepad, axis), pad.get(axis).unwrap_or(0.0));
                    }
                }
                None
            }
            GamepadEvent::Button(event) => {
                let previous = buttons
                    .insert((event.entity, event.button), event.value)
                    .unwrap_or(0.0);
                (event.value >= 0.55 && previous < 0.55).then_some(event.entity)
            }
            GamepadEvent::Axis(event) => {
                let previous = axes.entry((event.entity, event.axis)).or_insert(0.0);
                let deadzone = match event.axis {
                    GamepadAxis::LeftStickX | GamepadAxis::LeftStickY => settings.pad_deadzone_left,
                    _ => settings.pad_deadzone_right,
                }
                .max(0.15);
                let meaningful =
                    event.value.abs() > deadzone + 0.08 && (event.value - *previous).abs() > 0.12;
                if meaningful || event.value.abs() < deadzone {
                    *previous = event.value;
                }
                meaningful.then_some(event.entity)
            }
        };
        if let Some(entity) = activity.filter(|entity| {
            devices.focused && !connected.contains(entity) && gamepads.contains(*entity)
        }) {
            active.0 = Some(entity);
            devices.pad_prompts = true;
        }
    }
    if active.0.is_none() {
        devices.pad_prompts = false;
    }
    devices.style = match settings.pad_prompts {
        1 => frame::PromptStyle::Xbox,
        2 => frame::PromptStyle::PlayStation,
        3 => frame::PromptStyle::Generic,
        _ => active
            .0
            .and_then(|entity| gamepads.get(entity).ok())
            .map_or(frame::PromptStyle::Generic, |(_, pad, name)| {
                let name = name.map_or("", |name| name.as_str()).to_ascii_lowercase();
                if pad.vendor_id() == Some(0x054c)
                    || name.contains("dualshock")
                    || name.contains("dualsense")
                    || name == "wireless controller"
                {
                    frame::PromptStyle::PlayStation
                } else if pad.vendor_id() == Some(0x045e)
                    || name.contains("xbox")
                    || name.contains("xinput")
                {
                    frame::PromptStyle::Xbox
                } else {
                    frame::PromptStyle::Generic
                }
            }),
    };
}

const REPEAT_DELAY: f32 = 0.4;
const REPEAT_EVERY: f32 = 0.12;

pub(crate) fn drive_menus_with_pad(
    gamepads: Query<&Gamepad>,
    active: Res<frame::ActivePad>,
    script_menus: Option<Res<hud::ScriptMenus>>,
    frontend: Res<frame::UnifiedFrontend>,
    native_menu: Res<frame::NativeGameMenu>,
    screen: Res<frame::AppScreen>,
    (devices, console, capture): (
        Res<frame::InputDevices>,
        Res<crate::ConsoleState>,
        Res<frame::UiBindingCapture>,
    ),
    time: Res<Time>,
    mut requests: MessageWriter<UiMenuRequest>,
    mut repeat: Local<Option<(UiMenuKey, f32)>>,
) {
    if !devices.focused || console.open {
        *repeat = None;
        return;
    }
    let pad = active
        .0
        .and_then(|entity| gamepads.get(entity).ok())
        .filter(|_| capture.command.is_none());
    let Some(pad) = pad else {
        *repeat = None;
        return;
    };
    if pad.just_pressed(GamepadButton::Start) {
        requests.write(UiMenuRequest::Key(UiMenuKey::Escape));
        *repeat = None;
        return;
    }
    if !native_menu.0
        && !(frontend.0 && *screen == frame::AppScreen::MainMenu)
        && !script_menus.is_some_and(|menus| menus.captures_input())
    {
        *repeat = None;
        return;
    }
    if pad.just_pressed(GamepadButton::South) {
        requests.write(UiMenuRequest::Key(UiMenuKey::Enter));
    }
    if pad.just_pressed(GamepadButton::East) {
        requests.write(UiMenuRequest::Key(UiMenuKey::Escape));
    }
    let stick = pad.left_stick();
    let direction = if pad.pressed(GamepadButton::DPadUp) || stick.y > 0.6 {
        Some(UiMenuKey::Up)
    } else if pad.pressed(GamepadButton::DPadDown) || stick.y < -0.6 {
        Some(UiMenuKey::Down)
    } else if pad.pressed(GamepadButton::DPadLeft) || stick.x < -0.6 {
        Some(UiMenuKey::Left)
    } else if pad.pressed(GamepadButton::DPadRight) || stick.x > 0.6 {
        Some(UiMenuKey::Right)
    } else {
        None
    };
    let now = time.elapsed_secs();
    let fire = match (direction, *repeat) {
        (Some(key), Some((held, next))) if key == held => {
            if now >= next {
                *repeat = Some((key, now + REPEAT_EVERY));
            }
            now >= next
        }
        (Some(key), _) => {
            *repeat = Some((key, now + REPEAT_DELAY));
            true
        }
        (None, _) => {
            *repeat = None;
            false
        }
    };
    if fire && let Some(key) = direction {
        requests.write(UiMenuRequest::Key(key));
    }
}
