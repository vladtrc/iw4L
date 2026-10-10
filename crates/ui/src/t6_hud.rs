use asset_core::AssetNamespace;
use assets::{PreparedLocalizedStrings, PreparedWeapons, SessionMapIdentity};
use bevy::prelude::*;
use frame::{AppScreen, ClientSet};
use net::{ClientActionInput, FrameClock, LocalPresentClient, PresentedSnapshot};
use sim::{ClientId, ClientLifecycle, MatchPhase, Snapshot};
use std::collections::VecDeque;

use crate::layers::{GameUiFont, UiLayer, game_text_font};

#[derive(Component)]
struct T6HudRoot;

#[derive(Component)]
struct CrosshairArm(u8);
#[derive(Component)]
struct WeaponArt;
#[derive(Component)]
struct DamageEdge;

#[derive(Component, Clone, Copy)]
enum HudArt {
    Round,
    Score,
    Damage,
    Perk(u8),
    Powerup(u8),
    Tool,
}

#[derive(Component, Clone, Copy)]
enum Field {
    Match,
    Round,
    Use,
    Weapon,
    Ammo,
    Health,
    Status,
    Killfeed,
    Scoreboard,
    Powerups,
}

#[derive(Resource, Default)]
struct Killfeed {
    generation: frame::WorldGeneration,
    lines: VecDeque<(i32, String)>,
}

pub(crate) fn register(app: &mut App) {
    app.init_resource::<Killfeed>()
        .add_observer(obituary)
        .add_systems(
            Update,
            (spawn, refresh, refresh_graphics, refresh_art)
                .chain()
                .in_set(ClientSet::Ui)
                .in_set(super::t6_text::NativeUiPaint),
        );
}

fn obituary(
    event: On<net::EntityObituary>,
    map: Option<Res<SessionMapIdentity>>,
    presented: Res<PresentedSnapshot>,
    clock: Res<FrameClock>,
    mut feed: ResMut<Killfeed>,
) {
    if event.in_killcam || !map.is_some_and(|map| map.namespace == Some(AssetNamespace::T6)) {
        return;
    }
    let Some(snapshot) = presented.snapshot() else {
        return;
    };
    if feed.generation != event.event.world {
        feed.generation = event.event.world;
        feed.lines.clear();
    }
    let payload = event.event.payload;
    let victim = player_name(snapshot, ClientId(payload.other_entity_num as u32));
    let line = if payload.attacker_entity_num < 0
        || payload.attacker_entity_num == payload.other_entity_num
    {
        format!("{victim} died")
    } else {
        format!(
            "{}  >  {victim}",
            player_name(snapshot, ClientId(payload.attacker_entity_num as u32))
        )
    };
    feed.lines.push_back((clock.time(), line));
    while feed.lines.len() > 5 {
        feed.lines.pop_front();
    }
}

fn spawn(mut commands: Commands, font: Res<GameUiFont>, existing: Query<Entity, With<T6HudRoot>>) {
    if !existing.is_empty() {
        return;
    }
    commands
        .spawn((
            T6HudRoot,
            super::t6_text::NativeUiRoot(true),
            UiLayer::Hud,
            Visibility::Hidden,
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                display: Display::None,
                ..default()
            },
        ))
        .with_children(|root| {
            for (kind, left, right, bottom, width, height) in [
                (
                    HudArt::Score,
                    Val::Px(32.0),
                    Val::Auto,
                    Val::Px(30.0),
                    Val::Px(180.0),
                    Val::Px(48.0),
                ),
                (
                    HudArt::Round,
                    Val::Px(32.0),
                    Val::Auto,
                    Val::Px(104.0),
                    Val::Px(82.0),
                    Val::Px(70.0),
                ),
                (
                    HudArt::Tool,
                    Val::Px(182.0),
                    Val::Auto,
                    Val::Px(39.0),
                    Val::Px(30.0),
                    Val::Px(30.0),
                ),
                (
                    HudArt::Damage,
                    Val::Px(0.0),
                    Val::Px(0.0),
                    Val::Px(0.0),
                    Val::Percent(100.0),
                    Val::Percent(100.0),
                ),
                (
                    HudArt::Perk(0),
                    Val::Px(32.0),
                    Val::Auto,
                    Val::Px(180.0),
                    Val::Px(38.0),
                    Val::Px(38.0),
                ),
                (
                    HudArt::Perk(1),
                    Val::Px(76.0),
                    Val::Auto,
                    Val::Px(180.0),
                    Val::Px(38.0),
                    Val::Px(38.0),
                ),
                (
                    HudArt::Perk(2),
                    Val::Px(120.0),
                    Val::Auto,
                    Val::Px(180.0),
                    Val::Px(38.0),
                    Val::Px(38.0),
                ),
                (
                    HudArt::Perk(3),
                    Val::Px(164.0),
                    Val::Auto,
                    Val::Px(180.0),
                    Val::Px(38.0),
                    Val::Px(38.0),
                ),
                (
                    HudArt::Powerup(0),
                    Val::Percent(45.0),
                    Val::Auto,
                    Val::Px(60.0),
                    Val::Px(48.0),
                    Val::Px(48.0),
                ),
                (
                    HudArt::Powerup(1),
                    Val::Percent(50.0),
                    Val::Auto,
                    Val::Px(60.0),
                    Val::Px(48.0),
                    Val::Px(48.0),
                ),
            ] {
                root.spawn((
                    kind,
                    ImageNode {
                        image_mode: bevy::ui::widget::NodeImageMode::Stretch,
                        ..default()
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left,
                        right,
                        bottom,
                        width,
                        height,
                        display: Display::None,
                        ..default()
                    },
                ));
            }
            for (field, left, top, right, bottom, size) in [
                (
                    Field::Match,
                    Val::Px(28.0),
                    Val::Auto,
                    Val::Auto,
                    Val::Px(32.0),
                    30.0,
                ),
                (
                    Field::Round,
                    Val::Px(28.0),
                    Val::Auto,
                    Val::Auto,
                    Val::Px(110.0),
                    40.0,
                ),
                (
                    Field::Use,
                    Val::Percent(30.0),
                    Val::Percent(58.0),
                    Val::Auto,
                    Val::Auto,
                    20.0,
                ),
                (
                    Field::Weapon,
                    Val::Auto,
                    Val::Auto,
                    Val::Px(28.0),
                    Val::Px(88.0),
                    16.0,
                ),
                (
                    Field::Ammo,
                    Val::Auto,
                    Val::Auto,
                    Val::Px(32.0),
                    Val::Px(34.0),
                    36.0,
                ),
                (
                    Field::Health,
                    Val::Px(28.0),
                    Val::Auto,
                    Val::Auto,
                    Val::Px(112.0),
                    16.0,
                ),
                (
                    Field::Status,
                    Val::Percent(36.0),
                    Val::Percent(38.0),
                    Val::Auto,
                    Val::Auto,
                    26.0,
                ),
                (
                    Field::Killfeed,
                    Val::Px(28.0),
                    Val::Auto,
                    Val::Auto,
                    Val::Px(110.0),
                    18.0,
                ),
                (
                    Field::Scoreboard,
                    Val::Percent(20.0),
                    Val::Percent(15.0),
                    Val::Auto,
                    Val::Auto,
                    16.0,
                ),
                (
                    Field::Powerups,
                    Val::Percent(40.0),
                    Val::Auto,
                    Val::Auto,
                    Val::Px(60.0),
                    18.0,
                ),
            ] {
                root.spawn((
                    field,
                    Text::new(""),
                    game_text_font(&font.0, size),
                    TextColor(Color::srgb(0.95, 0.95, 0.95)),
                    TextShadow::default(),
                    BorderColor::all(Color::NONE),
                    BackgroundColor(if matches!(field, Field::Scoreboard) {
                        Color::srgba(0.025, 0.03, 0.04, 0.88)
                    } else {
                        Color::NONE
                    }),
                    Node {
                        position_type: PositionType::Absolute,
                        left,
                        top,
                        right,
                        bottom,
                        padding: UiRect::axes(Val::Px(12.0), Val::Px(8.0)),
                        border: if matches!(field, Field::Scoreboard) {
                            UiRect::bottom(Val::Px(2.0))
                        } else {
                            UiRect::ZERO
                        },
                        ..default()
                    },
                ));
            }
            root.spawn((
                WeaponArt,
                ImageNode::default(),
                Node {
                    position_type: PositionType::Absolute,
                    right: Val::Px(40.0),
                    bottom: Val::Px(114.0),
                    width: Val::Px(128.0),
                    height: Val::Px(48.0),
                    display: Display::None,
                    ..default()
                },
            ));
            for arm in 0..4 {
                root.spawn((
                    CrosshairArm(arm),
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Percent(50.0),
                        top: Val::Percent(50.0),
                        ..default()
                    },
                    BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.85)),
                    UiTransform::default(),
                ));
            }
            root.spawn((
                DamageEdge,
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    border: UiRect::all(Val::Px(4.0)),
                    ..default()
                },
                BorderColor::all(Color::NONE),
            ));
        });
}

fn player_name(snapshot: &Snapshot, id: ClientId) -> String {
    snapshot.meta.for_client(id).map_or_else(
        || format!("Player {}", id.0),
        |meta| {
            let end = meta
                .name
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(meta.name.len());
            let name = String::from_utf8_lossy(&meta.name[..end]);
            if name.is_empty() {
                format!("Player {}", id.0)
            } else {
                name.into_owned()
            }
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn refresh(
    map: Option<Res<SessionMapIdentity>>,
    screen: Res<AppScreen>,
    menu: Res<frame::NativeGameMenu>,
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    weapons: Option<Res<PreparedWeapons>>,
    strings: Option<Res<PreparedLocalizedStrings>>,
    actions: Option<Res<ClientActionInput>>,
    generation: Res<frame::WorldGeneration>,
    clock: Res<FrameClock>,
    mut feed: ResMut<Killfeed>,
    mut roots: Query<(&mut Node, &mut Visibility), (With<T6HudRoot>, Without<Field>)>,
    mut fields: Query<(&Field, &mut Text, &mut TextColor, &mut Node)>,
) {
    if feed.generation != *generation {
        feed.generation = *generation;
        feed.lines.clear();
    }
    feed.lines
        .retain(|(at, _)| (clock.time().wrapping_sub(*at) as u32) < 6_000);
    let visible = map
        .as_ref()
        .is_some_and(|map| map.namespace == Some(AssetNamespace::T6))
        && *screen == AppScreen::InGame
        && !menu.0
        && presented.snapshot().is_some();
    for (mut node, mut visibility) in &mut roots {
        *visibility = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        node.display = if visible {
            Display::Flex
        } else {
            Display::None
        };
    }
    if !visible {
        return;
    }
    let Some(snapshot) = presented.snapshot() else {
        return;
    };
    let Some(ps) = presented.player(local.0) else {
        return;
    };
    let Some(meta) = snapshot.meta.for_client(local.0) else {
        return;
    };
    let ended = meta.lifecycle == ClientLifecycle::Intermission
        || snapshot.meta.phase == MatchPhase::PostGame;
    let alive = meta.lifecycle == ClientLifecycle::Alive && ps.health > 0;
    let scores = ended || actions.as_ref().is_some_and(|a| a.client.kb.scores.active);
    let weapon = weapon_iw4::get_viewmodel_weapon_index(ps);
    let registry = weapons
        .as_ref()
        .and_then(|w| w.for_snapshot(presented.weapon_epoch()).ok())
        .map(|w| w.registry());
    let name = registry.as_ref().map_or_else(String::new, |registry| {
        registry
            .display_name_key_of(weapon)
            .and_then(|key| strings.as_ref()?.0.text_in(AssetNamespace::T6, key))
            .unwrap_or_else(|| registry.name_of(weapon))
            .to_owned()
    });
    let (clip, stock) = meta
        .ammo_by_weapon
        .iter()
        .find(|(index, _, _)| *index == weapon)
        .map_or((meta.ammo_clip, meta.ammo_stock), |(_, clip, stock)| {
            (*clip, *stock)
        });
    let time = if snapshot.meta.time_limit_ms == 0 {
        snapshot.meta.match_elapsed_ms
    } else {
        snapshot
            .meta
            .time_limit_ms
            .saturating_sub(snapshot.meta.match_elapsed_ms)
    } / 1000;
    let feed = feed
        .lines
        .iter()
        .map(|(_, line)| line.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let scoreboard = if scores {
        let mut rows = snapshot.meta.clients.iter().collect::<Vec<_>>();
        rows.sort_by_key(|(id, m)| {
            (
                if snapshot.meta.kind.is_team() {
                    m.client_state_team
                } else {
                    0
                },
                std::cmp::Reverse(m.score),
                id.0,
            )
        });
        let mut text = format!(
            "{}\nPlayer       Score / Kills / Deaths\n",
            snapshot.meta.kind.display_name()
        );
        let mut team = 0;
        for (id, m) in rows {
            if snapshot.meta.kind.is_team() && m.client_state_team != team {
                team = m.client_state_team;
                text.push_str(match team {
                    entity_iw4::TEAM_AXIS => "\nTEAM ORANGE\n",
                    entity_iw4::TEAM_ALLIES => "\nTEAM BLUE\n",
                    _ => "\nSPECTATORS\n",
                });
            }
            text.push_str(&format!(
                "{}   {} / {} / {}\n",
                player_name(snapshot, *id),
                m.score,
                m.kills,
                m.deaths
            ));
        }
        text
    } else {
        String::new()
    };
    for (field, mut text, mut color, mut node) in &mut fields {
        let value = match field {
            Field::Match => {
                let score = if snapshot.meta.kind.token() == "zclassic" {
                    meta.score.to_string()
                } else if snapshot.meta.kind.is_team() {
                    let team = usize::try_from(meta.client_state_team)
                        .ok()
                        .filter(|team| (1..=2).contains(team));
                    let own = team.map_or(0, |team| snapshot.meta.objectives.scores[team]);
                    let enemy = team.map_or(0, |team| snapshot.meta.objectives.scores[3 - team]);
                    format!("{own}  :  {enemy}     LIMIT {}", snapshot.meta.score_limit)
                } else {
                    format!("{}     LIMIT {}", meta.score, snapshot.meta.score_limit)
                };
                if snapshot.meta.kind.token() == "zclassic" {
                    score
                } else {
                    format!(
                        "{}   {:02}:{:02}\n{score}",
                        snapshot.meta.kind.display_name(),
                        time / 60,
                        time % 60
                    )
                }
            }
            Field::Round | Field::Use | Field::Powerups
                if snapshot.meta.kind.token() == "zclassic" && alive && !scores =>
            {
                let message = meta
                    .hud_archival
                    .iter()
                    .chain(&meta.hud_current)
                    .filter_map(|elem| {
                        sim::hud_string_in_occupied(&snapshot.meta.hud_strings, elem.text)
                    })
                    .filter_map(|raw| raw.strip_prefix(sim::HUD_STRING_PLAIN))
                    .find(|text| match field {
                        Field::Round => text.starts_with("ROUND "),
                        Field::Powerups => {
                            text.contains("DOUBLE POINTS") || text.contains("INSTA-KILL")
                        }
                        _ => {
                            text.starts_with("USE:")
                                || text.starts_with("Mystery Box")
                                || text.starts_with("Perk:")
                                || text.starts_with("Generator ")
                                || *text == "Another generator is being captured"
                        }
                    })
                    .unwrap_or("")
                    .to_owned();
                if matches!(field, Field::Use) {
                    let raw_name = message
                        .strip_prefix("USE: ")
                        .and_then(|rest| rest.split_whitespace().next());
                    if let Some(raw_name) = raw_name
                        && let Some(registry) = registry.as_ref()
                        && let Ok(Some(index)) =
                            registry.resolve_index(&format!("t6:weapon/{raw_name}"))
                        && let Some(key) = registry.display_name_key_of(index)
                        && let Some(label) = strings
                            .as_ref()
                            .and_then(|strings| strings.0.text_in(AssetNamespace::T6, key))
                    {
                        message.replacen(raw_name, label, 1)
                    } else {
                        message
                    }
                } else {
                    message
                        .strip_prefix("ROUND ")
                        .unwrap_or(&message)
                        .to_owned()
                }
            }
            Field::Weapon if alive && !scores => name.clone(),
            Field::Ammo if alive && !scores => format!("{clip:02}  /  {stock:03}"),
            Field::Health if alive && !scores && ps.health < ps.max_health => {
                format!("HEALTH {}", ps.health)
            }
            Field::Status if ended && snapshot.meta.kind.is_team() => {
                let team = usize::try_from(meta.client_state_team)
                    .ok()
                    .filter(|team| (1..=2).contains(team));
                team.map_or_else(
                    || "Match complete".into(),
                    |team| {
                        match snapshot.meta.objectives.scores[team]
                            .cmp(&snapshot.meta.objectives.scores[3 - team])
                        {
                            std::cmp::Ordering::Greater => "VICTORY",
                            std::cmp::Ordering::Less => "DEFEAT",
                            std::cmp::Ordering::Equal => "DRAW",
                        }
                        .into()
                    },
                )
            }
            Field::Status if ended && snapshot.meta.kind.token() == "zclassic" => {
                "GAME OVER".into()
            }
            Field::Status if alive && ps.pm_flags & playerstate_iw4::pm_flags::LAST_STAND != 0 => {
                "DOWNED - waiting for revive".into()
            }
            Field::Status if ended => "Match complete".into(),
            Field::Status
                if ps.other_flags & playerstate_iw4::other_flags::DEAD_KILLCAM_TPV != 0 =>
            {
                "KILLCAM\nHold use to respawn".into()
            }
            Field::Status if !alive => "Respawning...".into(),
            Field::Killfeed if !scores => feed.clone(),
            Field::Scoreboard => scoreboard.clone(),
            _ => String::new(),
        };
        node.display = if value.is_empty() {
            Display::None
        } else {
            Display::Flex
        };
        if text.0 != value {
            text.0 = value;
        }
        color.0 = if (matches!(field, Field::Health) && ps.health < ps.max_health / 2)
            || (matches!(field, Field::Ammo) && clip < 5)
        {
            Color::srgb(1.0, 0.25, 0.18)
        } else {
            Color::srgb(0.95, 0.95, 0.95)
        };
    }
}

fn refresh_art(
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    actions: Option<Res<ClientActionInput>>,
    mut art: ResMut<crate::t6_art::T6Art>,
    mut images: ResMut<Assets<Image>>,
    mut sprites: Query<(&HudArt, &mut ImageNode, &mut Node), Without<Field>>,
    mut fields: Query<(&Field, &mut Node), Without<HudArt>>,
    mut damage_edges: Query<&mut BorderColor, With<DamageEdge>>,
) {
    let Some(snapshot) = presented.snapshot() else {
        return;
    };
    let Some(ps) = presented.player(local.0) else {
        return;
    };
    let Some(meta) = snapshot.meta.for_client(local.0) else {
        return;
    };
    let alive = meta.lifecycle == ClientLifecycle::Alive && ps.health > 0;
    let scores = actions
        .as_ref()
        .is_some_and(|actions| actions.client.kb.scores.active);
    let zombies = snapshot.meta.kind.token() == "zclassic";
    let labels: Vec<_> = meta
        .hud_archival
        .iter()
        .chain(&meta.hud_current)
        .filter_map(|elem| sim::hud_string_in_occupied(&snapshot.meta.hud_strings, elem.text))
        .filter_map(|raw| raw.strip_prefix(sim::HUD_STRING_PLAIN))
        .collect();
    let tool = meta
        .hud_archival
        .iter()
        .chain(&meta.hud_current)
        .filter(|elem| elem.elem_type == hud_iw4::HE_TYPE_MATERIAL)
        .filter_map(|elem| {
            sim::name_in_occupied(
                &snapshot.meta.hud_materials,
                u8::try_from(elem.material_index).ok()?,
            )
        })
        .find(|name| {
            matches!(
                *name,
                "zom_hud_craftable_tank_shovel" | "zom_hud_shovel_gold"
            )
        });
    let round = labels
        .iter()
        .find_map(|label| label.strip_prefix("ROUND ")?.parse::<u32>().ok());
    let perks: Vec<_> = labels
        .iter()
        .find_map(|label| label.strip_prefix("PERKS: "))
        .unwrap_or("")
        .split_whitespace()
        .collect();
    let powerups = labels
        .iter()
        .find(|label| label.contains("DOUBLE POINTS") || label.contains("INSTA-KILL"))
        .copied()
        .unwrap_or("");
    let double_points = powerups.contains("DOUBLE POINTS");
    let insta_kill = powerups.contains("INSTA-KILL");
    let double_art = double_points
        .then(|| art.image("specialty_doublepoints_zombies", &mut images))
        .flatten();
    let instant_art = insta_kill
        .then(|| art.image("specialty_instakill_zombies", &mut images))
        .flatten();
    let powerups_rendered =
        (!double_points || double_art.is_some()) && (!insta_kill || instant_art.is_some());
    let tally = round
        .filter(|round| (1..=5).contains(round))
        .and_then(|round| art.image(&format!("hud_chalk_{round}"), &mut images));
    let damage = art.image("overlay_low_health", &mut images);
    if damage.is_some() {
        for mut edge in &mut damage_edges {
            *edge = BorderColor::all(Color::NONE);
        }
    }
    for (kind, mut view, mut node) in &mut sprites {
        let image = match kind {
            HudArt::Round if zombies => tally.clone(),
            HudArt::Tool if zombies => tool.and_then(|name| art.image(name, &mut images)),
            HudArt::Score => art.image(
                if zombies {
                    "scorebar_zom_1"
                } else {
                    "hud_mp_vis_left_lower_back"
                },
                &mut images,
            ),
            HudArt::Damage if ps.health < ps.max_health => damage.clone(),
            HudArt::Powerup(0) if zombies => double_art.clone(),
            HudArt::Powerup(1) if zombies => instant_art.clone(),
            HudArt::Perk(index)
                if zombies && ps.pm_flags & playerstate_iw4::pm_flags::LAST_STAND == 0 =>
            {
                perks
                    .get(*index as usize)
                    .and_then(|perk| match *perk {
                        "juggernog" => Some("specialty_juggernaut_zombies"),
                        "sleight" => Some("specialty_fastreload_zombies"),
                        "revive" => Some("specialty_quickrevive_zombies"),
                        "doubletap" => Some("specialty_doubletap_zombies"),
                        "staminup" => Some("specialty_marathon_zombies"),
                        "deadshot" => Some("specialty_ads_zombies"),
                        "mulekick" => Some("specialty_additionalprimaryweapon_zombies"),
                        "phd" => Some("specialty_divetonuke_zombies"),
                        _ => None,
                    })
                    .and_then(|material| art.image(material, &mut images))
            }
            _ => None,
        };
        node.display = if alive && !scores && image.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if let Some(image) = image {
            view.image = image;
        }
        view.color = if matches!(kind, HudArt::Damage) {
            Color::srgba(
                1.0,
                1.0,
                1.0,
                (1.0 - ps.health as f32 / ps.max_health.max(1) as f32).clamp(0.0, 0.85),
            )
        } else {
            Color::WHITE
        };
    }
    for (field, mut node) in &mut fields {
        if (matches!(field, Field::Round) && zombies && tally.is_some() && alive && !scores)
            || (matches!(field, Field::Health) && damage.is_some())
            || (matches!(field, Field::Powerups) && zombies && powerups_rendered)
        {
            node.display = Display::None;
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn refresh_graphics(
    presented: Res<PresentedSnapshot>,
    local: Res<LocalPresentClient>,
    actions: Option<Res<ClientActionInput>>,
    weapons: Option<Res<PreparedWeapons>>,
    catalog: Res<crate::ClassLoadoutCatalog>,
    mut art: ResMut<crate::t6_art::T6Art>,
    mut images: ResMut<Assets<Image>>,
    generation: Res<frame::WorldGeneration>,
    mut arms: Query<
        (&CrosshairArm, &mut Node, &mut UiTransform),
        (Without<WeaponArt>, Without<DamageEdge>),
    >,
    mut weapon_art: Query<
        (&mut ImageNode, &mut Node),
        (With<WeaponArt>, Without<CrosshairArm>, Without<DamageEdge>),
    >,
    mut damage: Query<&mut BorderColor, With<DamageEdge>>,
) {
    art.reset(*generation);
    let Some(ps) = presented.player(local.0) else {
        return;
    };
    let Some(snapshot) = presented.snapshot() else {
        return;
    };
    let Some(meta) = snapshot.meta.for_client(local.0) else {
        return;
    };
    let alive = meta.lifecycle == ClientLifecycle::Alive && ps.health > 0;
    let scores = actions.as_ref().is_some_and(|a| a.client.kb.scores.active);
    let crosshair = alive && !scores && ps.f_weapon_pos_frac < 0.5;
    let spread = 5.0 + ps.aim_spread_scale.clamp(0.0, 255.0) / 255.0 * 18.0;
    for (arm, mut node, mut transform) in &mut arms {
        node.display = if crosshair {
            Display::Flex
        } else {
            Display::None
        };
        let (x, y, w, h) = match arm.0 {
            0 => (-spread - 7.0, -1.0, 7.0, 2.0),
            1 => (spread, -1.0, 7.0, 2.0),
            2 => (-1.0, -spread - 7.0, 2.0, 7.0),
            _ => (-1.0, spread, 2.0, 7.0),
        };
        node.width = Val::Px(w);
        node.height = Val::Px(h);
        *transform = UiTransform::from_translation(Val2::px(x, y));
    }
    for mut border in &mut damage {
        *border = BorderColor::all(if alive && ps.health < ps.max_health / 2 {
            Color::srgba(0.8, 0.05, 0.02, 0.65)
        } else {
            Color::NONE
        });
    }
    let image = weapons.as_ref().and_then(|w| {
        let w = w.for_snapshot(presented.weapon_epoch()).ok()?;
        let index = weapon_iw4::get_viewmodel_weapon_index(ps);
        if w.registry().identity_namespace_of(index) != Some(AssetNamespace::T6) {
            return None;
        }
        let key = w
            .registry()
            .weapon_families()
            .describe(index)?
            .family
            .as_ref()?
            .asset_key();
        let preview = catalog.previews.get(&key);
        let material = preview.filter(|p| !p.image.is_empty())?.image.as_str();
        art.image(material, &mut images)
    });
    for (mut view, mut node) in &mut weapon_art {
        node.display = if alive && !scores && image.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        if let Some(image) = &image {
            if view.image != *image {
                view.image = image.clone();
            }
            if let Some(asset) = images.get(image) {
                node.height = Val::Px(
                    128.0 * asset.texture_descriptor.size.height as f32
                        / asset.texture_descriptor.size.width.max(1) as f32,
                );
            }
        }
    }
}
