use bevy::input::gamepad::GamepadButton;
use bevy::prelude::*;

use crate::browser::{Browser, BrowserButton, BrowserRoot};
use crate::editor::{EditorAction, EditorButton, EditorRoot, TheaterState, TimelineBar};

#[derive(Default)]
struct Navigation {
    selected: usize,
    focus: Option<Entity>,
    repeat: Option<(IVec2, f32)>,
    root: Option<Entity>,
    library: Option<bool>,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct TheaterNavigation;

pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        navigate
            .in_set(frame::ClientSet::Ui)
            .in_set(TheaterNavigation),
    );
}

#[derive(bevy::ecs::system::SystemParam)]
struct TheaterNavigationInput<'w, 's> {
    keys: Res<'w, ButtonInput<KeyCode>>,
    pads: Query<'w, 's, &'static Gamepad>,
    active: Res<'w, frame::ActivePad>,
    devices: Res<'w, frame::InputDevices>,
    console: Res<'w, console::ConsoleState>,
    overlay: Res<'w, frame::UiOverlayInput>,
}

#[derive(bevy::ecs::system::SystemParam)]
struct TheaterFocusNavigation<'w, 's> {
    commands: Commands<'w, 's>,
    #[expect(
        clippy::type_complexity,
        reason = "Bevy system parameters keep scheduler resource access explicit."
    )]
    roots: Query<'w, 's, Entity, Or<(With<BrowserRoot>, With<EditorRoot>)>>,
    children: Query<'w, 's, &'static Children>,
    #[expect(
        clippy::type_complexity,
        reason = "Bevy system parameters keep scheduler resource access explicit."
    )]
    buttons: Query<
        'w,
        's,
        (
            Entity,
            &'static UiGlobalTransform,
            Option<Ref<'static, bevy::picking::hover::Hovered>>,
            Option<&'static BrowserButton>,
            Option<&'static EditorButton>,
            Has<TimelineBar>,
        ),
        With<bevy::ui_widgets::Button>,
    >,
    nav: Local<'s, Navigation>,
}

impl TheaterNavigationInput<'_, '_> {
    fn blocked(&self) -> bool {
        !self.overlay.0 || !self.devices.focused || self.console.open
    }

    fn pad(&self) -> Option<&Gamepad> {
        self.active.0.and_then(|entity| self.pads.get(entity).ok())
    }
}

impl TheaterFocusNavigation<'_, '_> {
    fn navigate(
        mut self,
        input: TheaterNavigationInput,
        time: Res<Time>,
        mut browser: ResMut<Browser>,
        mut editor: ResMut<TheaterState>,
    ) {
        if input.blocked() {
            self.nav.repeat = None;
            return;
        }
        let pad = input.pad();
        if !browser.open {
            if input.keys.just_pressed(KeyCode::Escape)
                || pad.is_some_and(|p| p.just_pressed(GamepadButton::East))
            {
                editor.actions.push(EditorAction::Exit);
                return;
            }
            if let Some(pad) = pad {
                for (button, action) in [
                    (GamepadButton::Start, EditorAction::Controls),
                    (GamepadButton::Select, EditorAction::Tools),
                    (GamepadButton::North, EditorAction::Hud),
                    (GamepadButton::West, EditorAction::Pause),
                    (GamepadButton::LeftTrigger, EditorAction::Player(false)),
                    (GamepadButton::RightTrigger, EditorAction::Player(true)),
                    (GamepadButton::LeftTrigger2, EditorAction::Event(false)),
                    (GamepadButton::RightTrigger2, EditorAction::Event(true)),
                ] {
                    if pad.just_pressed(button) {
                        editor.actions.push(action);
                    }
                }
            }
        } else if pad.is_some_and(|p| {
            p.just_pressed(GamepadButton::East) || p.just_pressed(GamepadButton::Start)
        }) {
            browser.actions.push(BrowserButton::Back);
            return;
        }
        let Some(root) = self.roots.iter().next() else {
            return;
        };
        let mut ordered = Vec::new();
        let mut stack = vec![root];
        while let Some(entity) = stack.pop() {
            if self.buttons.contains(entity) {
                ordered.push(entity);
            }
            if let Ok(descendants) = self.children.get(entity) {
                stack.extend(descendants.iter().rev());
            }
        }
        if ordered.is_empty() {
            return;
        }
        if self.nav.library != Some(browser.open) {
            self.nav.library = Some(browser.open);
            self.nav.selected = 0;
        }
        self.nav.selected = self.nav.selected.min(ordered.len() - 1);
        if self.nav.root != Some(root) {
            self.nav.root = Some(root);
            self.nav.focus = None;
        }
        for (index, &entity) in ordered.iter().enumerate() {
            if let Ok((_, _, interaction, _, _, _)) = self.buttons.get(entity)
                && interaction.is_some_and(|hovered| hovered.is_changed() && hovered.get())
            {
                self.nav.selected = index;
            }
        }
        let stick = pad.map_or(Vec2::ZERO, Gamepad::left_stick);
        let held = |button| pad.is_some_and(|p| p.pressed(button));
        let direction = if held(GamepadButton::DPadUp) || stick.y > 0.6 {
            IVec2::NEG_Y
        } else if held(GamepadButton::DPadDown) || stick.y < -0.6 {
            IVec2::Y
        } else if held(GamepadButton::DPadLeft) || stick.x < -0.6 {
            IVec2::NEG_X
        } else if held(GamepadButton::DPadRight) || stick.x > 0.6 {
            IVec2::X
        } else {
            IVec2::ZERO
        };
        let now = time.elapsed_secs();
        let fire = if direction == IVec2::ZERO {
            self.nav.repeat = None;
            false
        } else if let Some((held, next)) = self.nav.repeat.filter(|(held, _)| *held == direction) {
            if now >= next {
                self.nav.repeat = Some((held, now + 0.12));
                true
            } else {
                false
            }
        } else {
            self.nav.repeat = Some((direction, now + 0.4));
            true
        };
        let mut direction = if fire {
            direction.as_vec2()
        } else {
            Vec2::ZERO
        };
        if browser.open {
            for (key, vector) in [
                (KeyCode::ArrowUp, Vec2::NEG_Y),
                (KeyCode::ArrowDown, Vec2::Y),
                (KeyCode::ArrowLeft, Vec2::NEG_X),
                (KeyCode::ArrowRight, Vec2::X),
            ] {
                if input.keys.just_pressed(key) {
                    direction = vector;
                }
            }
        }
        let current = self.buttons.get(ordered[self.nav.selected]).unwrap();
        if current.5 && direction.x != 0.0 {
            editor
                .actions
                .push(EditorAction::Skip(if direction.x < 0.0 {
                    -100
                } else {
                    100
                }));
        } else if direction != Vec2::ZERO {
            let origin = current.1.translation;
            if let Some((index, _)) = ordered
                .iter()
                .enumerate()
                .filter_map(|(index, &entity)| {
                    let position = self.buttons.get(entity).ok()?.1.translation;
                    let delta = position - origin;
                    let forward = delta.dot(direction);
                    (forward > 1.0)
                        .then_some((index, forward + delta.perp_dot(direction).abs() * 3.0))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
            {
                self.nav.selected = index;
            }
        }
        if input.keys.just_pressed(KeyCode::Tab) {
            self.nav.selected = if input.keys.pressed(KeyCode::ShiftLeft)
                || input.keys.pressed(KeyCode::ShiftRight)
            {
                (self.nav.selected + ordered.len() - 1) % ordered.len()
            } else {
                (self.nav.selected + 1) % ordered.len()
            };
        }
        let entity = ordered[self.nav.selected];
        if self.nav.focus != Some(entity) {
            if let Some(previous) = self.nav.focus
                && let Ok(mut previous) = self.commands.get_entity(previous)
            {
                previous.try_remove::<Outline>();
            }
            self.commands.entity(entity).try_insert(Outline {
                width: Val::Px(3.0),
                offset: Val::Px(1.0),
                color: Color::srgb(0.8, 0.85, 0.35),
            });
            self.nav.focus = Some(entity);
        }
        if input.keys.just_pressed(KeyCode::Enter)
            || pad.is_some_and(|p| p.just_pressed(GamepadButton::South))
        {
            let (_, _, _, browser_button, editor_button, timeline) =
                self.buttons.get(entity).unwrap();
            if let Some(button) = browser_button {
                browser.actions.push(button.clone());
            } else if let Some(button) = editor_button {
                editor.actions.push(button.0.clone());
            } else if timeline {
                editor.actions.push(EditorAction::Pause);
            }
        }
    }
}

fn navigate(
    navigation: TheaterFocusNavigation,
    input: TheaterNavigationInput,
    time: Res<Time>,
    browser: ResMut<Browser>,
    editor: ResMut<TheaterState>,
) {
    navigation.navigate(input, time, browser, editor);
}
