use bevy::prelude::*;

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct CommunityServerPicker<'w, 's> {
    dvars: ResMut<'w, frame::UiMenuDvars>,
    choice: Local<'s, Option<String>>,
    error: Local<'s, Option<String>>,
    restarting: Local<'s, bool>,
    population: Local<'s, Vec<Option<u32>>>,
}

impl CommunityServerPicker<'_, '_> {
    fn update(
        mut self,
        mut events: MessageReader<crate::ConsoleCommand>,
        servers: Res<ui::CommunityServers>,
        counts: Option<Res<net::CommunityPlayerCounts>>,
        (screen, party): (Res<frame::AppScreen>, Res<frame::UiPartyState>),
        mut exit: MessageWriter<AppExit>,
        mut echo: crate::feature_dispatch::ConsoleEcho,
    ) {
        let mut refresh = self.dvars.choices("ui_community_server").is_none();
        if let Some(counts) = counts.as_ref().and_then(|counts| counts.snapshot())
            && counts != *self.population
        {
            *self.population = counts;
            refresh = true;
        }
        let counts = &*self.population;
        let online = servers
            .selected
            .parse::<usize>()
            .ok()
            .and_then(|index| counts.get(index))
            .copied()
            .flatten();
        self.dvars.set(
            "ui_players_online",
            online.map_or_else(
                || "Players online: unavailable".to_owned(),
                |count| format!("{count} players online"),
            ),
        );
        if refresh {
            self.dvars
                .set_choices("ui_community_server", servers.display_choices(counts));
        }
        let choice = self.choice.get_or_insert_with(|| servers.selected.clone());
        let in_menu = *screen == frame::AppScreen::MainMenu;
        let locked =
            !in_menu || party.in_lobby || servers.choices.iter().all(|(_, value)| value.is_empty());
        for command in events.read() {
            if !matches!(command.name.as_str(), "set" | "seta") {
                continue;
            }
            let [name, value, ..] = command.args.as_slice() else {
                continue;
            };
            match name.as_str() {
                "ui_community_server" if !locked => {
                    if servers.choices.iter().any(|(_, option)| option == value) {
                        *choice = value.clone();
                        *self.error = None;
                    }
                }
                "ui_community_apply" if value == "1" && !*self.restarting => {
                    if locked {
                        echo.write(
                            "Leave the lobby and return to the main menu before switching servers.",
                        );
                    } else if !choice.is_empty() && *choice != servers.selected {
                        match servers.restart(choice) {
                            Ok(()) => {
                                *self.restarting = true;
                                diag::lifecycle_boundary("quit_requested", " via=community_switch");
                                exit.write(AppExit::Success);
                            }
                            Err(cause) => {
                                echo.write(format!("Community switch: {cause}"));
                                *self.error = Some(cause);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        self.dvars
            .set("ui_community_menu", u8::from(in_menu).to_string());
        self.dvars.set("ui_community_server", choice.clone());
        self.dvars
            .set("ui_community_locked", u8::from(locked).to_string());
        self.dvars.set(
            "ui_community_apply_disabled",
            u8::from(
                locked || choice.is_empty() || *choice == servers.selected || *self.restarting,
            )
            .to_string(),
        );
        let current = servers
            .choices
            .iter()
            .find(|(_, value)| *value == servers.selected && !value.is_empty())
            .map(|(label, _)| label.as_str())
            .unwrap_or("None");
        self.dvars
            .set("ui_community_current", format!("Current server: {current}"));
        let hint = if let Some(error) = self.error.as_deref() {
            error
        } else if party.in_lobby {
            "Leave your lobby before switching servers."
        } else if servers.choices.iter().all(|(_, value)| value.is_empty()) {
            "Add .iw4l-server files beside the game, then restart."
        } else {
            "Applying a server restarts the game and checks its updates."
        };
        self.dvars.set("ui_community_hint", hint);
    }
}

pub(crate) fn community_server_menu(
    picker: CommunityServerPicker,
    events: MessageReader<crate::ConsoleCommand>,
    servers: Res<ui::CommunityServers>,
    counts: Option<Res<net::CommunityPlayerCounts>>,
    menu: (Res<frame::AppScreen>, Res<frame::UiPartyState>),
    exit: MessageWriter<AppExit>,
    echo: crate::feature_dispatch::ConsoleEcho,
) {
    picker.update(events, servers, counts, menu, exit, echo);
}
