use crate::{BindButton, ConsoleCommand, ConsoleSettings, ConsoleState, KeyBinds};
use bevy::ecs::system::SystemParam;
use bevy::{
    input::{ButtonState, keyboard::KeyboardInput},
    prelude::*,
};
use frame::ScopeApp;

#[derive(Resource, Default)]
pub(crate) struct ChatState {
    pub open: bool,
    team_only: bool,
    line: String,
    caret: usize,
    captured: bool,
    error: Option<String>,
    recent: std::collections::VecDeque<(f64, String, String, i32)>,
}

impl ChatState {
    pub fn captures_input(&self) -> bool {
        self.open || self.captured
    }
    fn begin(&mut self, team_only: bool) {
        self.open = true;
        self.team_only = team_only;
        self.line.clear();
        self.caret = 0;
        self.captured = true;
        self.error = None;
    }
}

#[derive(Component)]
struct ChatText(usize);

#[derive(Component)]
struct ChatBody(usize);

#[derive(Component)]
struct ChatPrompt;

pub(crate) fn install(app: &mut App) {
    app.scoped::<ChatState>(frame::MatchScope::Live)
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (commands, display).chain().in_set(net::ClientSet::Diag),
        );
}

fn setup(mut commands: Commands, font: Res<ui::GameUiFont>) {
    commands
        .spawn((
            ui::UiLayer::Hud,
            Node {
                position_type: PositionType::Absolute,
                left: px(12),
                top: percent(25),
                width: px(300),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            GlobalZIndex(20),
        ))
        .with_children(|parent| {
            for index in 0..5 {
                parent
                    .spawn((
                        ChatText(index),
                        Text::new(""),
                        ui::game_text_font(&font.0, 12.0),
                        TextColor(Color::WHITE),
                        TextShadow {
                            offset: Vec2::splat(1.0),
                            color: Color::BLACK,
                        },
                    ))
                    .with_children(|parent| {
                        parent.spawn((
                            ChatBody(index),
                            TextSpan::new(""),
                            ui::game_text_font(&font.0, 12.0),
                            TextColor(Color::WHITE),
                        ));
                    });
            }
            parent.spawn((
                ChatPrompt,
                Text::new(""),
                ui::game_text_font(&font.0, 12.0),
                TextColor(Color::WHITE),
                TextShadow {
                    offset: Vec2::splat(1.0),
                    color: Color::BLACK,
                },
            ));
        });
}

#[derive(SystemParam)]
pub(crate) struct ChatOutbox<'w> {
    local: Res<'w, net::LocalPresentClient>,
    inbox: ResMut<'w, net::ClientActionInbox>,
}

fn submit(state: &mut ChatState, outbox: &mut ChatOutbox) {
    if !state.line.trim().is_empty()
        && let Err(error) = outbox.inbox.push(
            outbox.local.0,
            sim::ClientAction::Chat {
                team_only: state.team_only,
                text: sim::chat::payload(&state.line),
            },
        )
    {
        state.error = Some(error.to_string());
        return;
    }
    state.open = false;
    state.line.clear();
    state.caret = 0;
}

#[expect(
    clippy::type_complexity,
    reason = "Bevy system parameters keep scheduler resource access explicit."
)]
pub(crate) fn keyboard(
    mut events: MessageReader<KeyboardInput>,
    state: Option<ResMut<ChatState>>,
    binds: Res<KeyBinds>,
    (console, menus, screen, playback): (
        Res<ConsoleState>,
        hud::MenuState<'_>,
        Res<frame::AppScreen>,
        Option<Res<replay::ReplayPlayback>>,
    ),
    (keys, mouse, pads, active, mut wheel): (
        Res<ButtonInput<KeyCode>>,
        Res<ButtonInput<MouseButton>>,
        Query<&bevy::input::gamepad::Gamepad>,
        Res<frame::ActivePad>,
        MessageReader<bevy::input::mouse::MouseWheel>,
    ),
    mut outbox: ChatOutbox,
) {
    let Some(mut state) = state else {
        events.clear();
        return;
    };
    state.captured = false;
    let available = *screen == frame::AppScreen::InGame && playback.is_none();
    let modal = console.open || menus.is_some_and(|menus| menus.captures_input());
    if !available || modal {
        state.open = false;
        state.line.clear();
        events.clear();
        return;
    }
    let wheel_direction = wheel.read().find_map(|event| {
        if event.y > 0.0 {
            Some(BindButton::WheelUp)
        } else if event.y < 0.0 {
            Some(BindButton::WheelDown)
        } else {
            None
        }
    });
    if !state.open {
        let inputs = crate::BindInputs::new(&keys, &mouse)
            .with_pad(active.0.and_then(|id| pads.get(id).ok()));
        for (button, id) in binds
            .iter()
            .filter(|(button, _)| !matches!(button, BindButton::Key(_)))
        {
            let team_only = match input_iw4::command_name(id) {
                Some("chatmodepublic") => false,
                Some("chatmodeteam") => true,
                _ => continue,
            };
            if inputs.just_pressed(button) || wheel_direction == Some(button) {
                state.begin(team_only);
                events.clear();
                return;
            }
        }
    }
    for event in events.read().filter(|e| e.state == ButtonState::Pressed) {
        if !state.open {
            if !event.repeat {
                match binds
                    .get(BindButton::Key(event.key_code))
                    .and_then(input_iw4::command_name)
                {
                    Some("chatmodepublic") => state.begin(false),
                    Some("chatmodeteam") => state.begin(true),
                    _ => {}
                }
            }
            continue;
        }
        state.captured = true;
        match event.key_code {
            KeyCode::Escape | KeyCode::Backquote => {
                state.open = false;
                state.line.clear();
            }
            KeyCode::Enter | KeyCode::NumpadEnter => submit(&mut state, &mut outbox),
            KeyCode::ArrowLeft => {
                state.caret = state.line[..state.caret]
                    .char_indices()
                    .last()
                    .map_or(0, |(i, _)| i);
            }
            KeyCode::ArrowRight => {
                state.caret += state.line[state.caret..]
                    .chars()
                    .next()
                    .map_or(0, char::len_utf8);
            }
            KeyCode::Home => state.caret = 0,
            KeyCode::End => state.caret = state.line.len(),
            KeyCode::Backspace if state.caret > 0 => {
                let end = state.caret;
                let start = state.line[..end].char_indices().last().unwrap().0;
                state.line.replace_range(start..end, "");
                state.caret = start;
            }
            KeyCode::Delete if state.caret < state.line.len() => {
                let start = state.caret;
                let end = start + state.line[start..].chars().next().unwrap().len_utf8();
                state.line.replace_range(start..end, "");
            }
            _ => {
                if let Some(text) = &event.text {
                    for ch in text.chars().filter(|c| !c.is_control()) {
                        if state.line.len() + ch.len_utf8() < sim::chat::CHAT_BYTES {
                            let caret = state.caret;
                            state.line.insert(caret, ch);
                            state.caret += ch.len_utf8();
                        }
                    }
                }
            }
        }
    }
}

fn commands(
    mut events: MessageReader<ConsoleCommand>,
    state: Option<ResMut<ChatState>>,
    (screen, playback): (Res<frame::AppScreen>, Option<Res<replay::ReplayPlayback>>),
    mut outbox: ChatOutbox,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
) {
    let Some(mut state) = state else {
        for cmd in events.read().filter(|c| {
            matches!(
                c.name.as_str(),
                "chatmodepublic" | "chatmodeteam" | "say" | "say_team"
            )
        }) {
            console.echo(
                format!("{}: join a match first", cmd.name),
                settings.log_capacity,
            );
        }
        return;
    };
    for cmd in events.read().filter(|c| {
        matches!(
            c.name.as_str(),
            "chatmodepublic" | "chatmodeteam" | "say" | "say_team"
        )
    }) {
        if *screen != frame::AppScreen::InGame || playback.is_some() {
            console.echo("chat: requires a live match", settings.log_capacity);
            continue;
        }
        let team = matches!(cmd.name.as_str(), "chatmodeteam" | "say_team");
        state.begin(team);
        if matches!(cmd.name.as_str(), "say" | "say_team") {
            state.line = sim::chat::clean(&cmd.args.join(" "));
            submit(&mut state, &mut outbox);
        } else {
            console.open = false;
        }
    }
}

fn display(
    mut messages: MessageReader<net::SvcGameNotify>,
    state: Option<ResMut<ChatState>>,
    time: Res<Time<Real>>,
    (screen, local, presented, strings): (
        Res<frame::AppScreen>,
        Res<net::LocalPresentClient>,
        Res<net::PresentedSnapshot>,
        Option<Res<assets::PreparedLocalizedStrings>>,
    ),
    mut names: Query<(&ChatText, &mut Text, &mut TextColor), Without<ChatPrompt>>,
    mut bodies: Query<(&ChatBody, &mut TextSpan)>,
    mut prompt: Query<&mut Text, (With<ChatPrompt>, Without<ChatText>)>,
) {
    let Some(mut state) = state else {
        messages.clear();
        for (_, mut text, _) in &mut names {
            if !text.0.is_empty() {
                text.0.clear();
            }
        }
        for (_, mut body) in &mut bodies {
            if !body.0.is_empty() {
                body.0.clear();
            }
        }
        if let Ok(mut text) = prompt.single_mut()
            && !text.0.is_empty()
        {
            text.0.clear();
        }
        return;
    };
    let now = time.elapsed_secs_f64();
    for msg in messages.read().filter(|m| m.tag == net::SVC_CHAT) {
        let (name, body) = msg
            .name
            .split_once(sim::HUD_PRINT_ARG_SEPARATOR)
            .unwrap_or(("", &msg.name));
        let (team, localized) = msg
            .key
            .split_once(':')
            .map_or((msg.key.as_str(), false), |(team, kind)| {
                (team, kind == "localized")
            });
        let body = if localized {
            let key = body.strip_prefix(": ").unwrap_or(body);
            format!(
                ": {}",
                strings.as_ref().and_then(|s| s.0.text(key)).unwrap_or(key)
            )
        } else {
            body.to_owned()
        };
        state
            .recent
            .push_back((now, name.to_owned(), body, team.parse().unwrap_or(0)));
        while state.recent.len() > 5 {
            state.recent.pop_front();
        }
    }
    while state.recent.front().is_some_and(|(at, ..)| now - at > 8.0) {
        state.recent.pop_front();
    }
    let in_game = *screen == frame::AppScreen::InGame;
    let local_team = presented
        .snapshot()
        .and_then(|s| s.meta.for_client(local.0))
        .map_or(0, |m| m.client_state_team);
    for (index, mut text, mut color) in &mut names {
        let row = state.recent.get(index.0).filter(|_| in_game);
        let name = row.map_or("", |(_, name, ..)| name.as_str());
        if text.0 != name {
            text.0 = name.to_owned();
        }
        let rgba = row.map_or([1.0; 4], |(_, _, _, team)| {
            presented.snapshot().map_or([1.0; 4], |s| {
                hud::team_name_color(local_team, *team, s.meta.script_dvars(local.0))
            })
        });
        let next = Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3]);
        if color.0 != next {
            color.0 = next;
        }
    }
    for (index, mut body) in &mut bodies {
        let next = state
            .recent
            .get(index.0)
            .filter(|_| in_game)
            .map_or("", |(_, _, body, _)| body.as_str());
        if body.0 != next {
            body.0 = next.to_owned();
        }
    }
    let Ok(mut text) = prompt.single_mut() else {
        return;
    };
    let next = if in_game && state.open {
        let label = if state.team_only { "Team" } else { "Say" };
        format!(
            "{label}: {}|{}{}",
            &state.line[..state.caret],
            &state.line[state.caret..],
            state
                .error
                .as_ref()
                .map_or(String::new(), |e| format!("\n{e}"))
        )
    } else {
        String::new()
    };
    if text.0 != next {
        text.0 = next;
    }
}
