use std::sync::Arc;

use crate::{
    CompiledAnimNode, CompiledAnimTreeDefinition, ParsedAnimCommand, ParsedAnimItem,
    ParsedPlayerAnimScript,
};

const MOVEMENT: &[(u8, &str)] = &[
    (1, "pb_stand_alert"),
    (2, "pb_crouch_alert"),
    (3, "pb_prone_aim"),
    (4, "pb_stand_shoot_walk_forward"),
    (5, "pb_stand_shoot_walk_back"),
    (6, "pb_crouch_shoot_run_forward"),
    (7, "pb_crouch_shoot_run_back"),
    (8, "pb_prone_crawl"),
    (9, "pb_prone_crawl_back"),
    (10, "pb_combatrun_forward_loop"),
    (11, "pb_combatrun_back_loop"),
    (12, "pb_crouch_run_forward"),
    (13, "pb_crouch_run_back"),
    (14, "pb_stand_alert"),
    (15, "pb_stand_alert"),
    (16, "pb_crouch_alert"),
    (17, "pb_crouch_alert"),
    (20, "pb_sprint"),
];

const EVENTS: &[(u8, u8, &str)] = &[
    (1, 3, "pb_death_flatonback"),
    (2, 2, "pt_rifle_fire"),
    (3, 3, "pb_standjump_takeoff"),
    (4, 3, "pb_standjump_takeoff"),
    (5, 3, "pb_standjump_land"),
    (10, 2, "pt_reload_stand_rifle"),
    (17, 2, "pt_knife_stand_melee_swipe"),
    (18, 2, "pt_knife_stand_melee_swipe"),
    (19, 2, "pt_knife_stand_melee_swipe"),
];

pub(crate) fn clip_names() -> impl Iterator<Item = &'static str> {
    MOVEMENT
        .iter()
        .map(|(_, name)| *name)
        .chain(EVENTS.iter().map(|(_, _, name)| *name))
}

pub(crate) fn compile() -> (Arc<CompiledAnimTreeDefinition>, Arc<ParsedPlayerAnimScript>) {
    let mut names: Vec<_> = clip_names().collect();
    names.sort_unstable();
    names.dedup();
    names.sort_by_key(|name| name.starts_with("pt_"));
    let legs_count = names.iter().filter(|name| !name.starts_with("pt_")).count();
    let mut nodes = vec![
        CompiledAnimNode {
            name: "t6_player".into(),
            parent: None,
            flags: 0,
            child_count: 2,
            first_child: 1,
        },
        CompiledAnimNode {
            name: "legs".into(),
            parent: Some(0),
            flags: 0,
            child_count: legs_count as u16,
            first_child: 3,
        },
        CompiledAnimNode {
            name: "torso".into(),
            parent: Some(0),
            flags: 0,
            child_count: (names.len() - legs_count) as u16,
            first_child: (3 + legs_count) as u16,
        },
    ];
    nodes.extend(names.iter().map(|name| CompiledAnimNode {
        name: (*name).into(),
        parent: Some(if name.starts_with("pt_") { 2 } else { 1 }),
        flags: 0,
        child_count: 0,
        first_child: 0,
    }));
    let item = |body_part, name| ParsedAnimItem {
        skip: false,
        conditions: Vec::new(),
        raw: "default".into(),
        commands: vec![ParsedAnimCommand {
            body_part,
            anim_index: (3 + names.iter().position(|n| *n == name).unwrap()) as u16,
            duration_ms: None,
            blend_ms: Some(120),
        }],
    };
    let slots: Vec<_> = MOVEMENT
        .iter()
        .map(|(movement, name)| (0, *movement, vec![item(3, *name)]))
        .collect();
    let events: Vec<_> = EVENTS
        .iter()
        .map(|(event, part, name)| (*event, vec![item(*part, *name)]))
        .collect();
    let count = slots.len() + events.len();
    let script = ParsedPlayerAnimScript {
        slots,
        events,
        item_count: MOVEMENT.len(),
        skipped_items: 0,
        command_count: count,
        unresolved_anims: 0,
        event_item_count: EVENTS.len(),
    };
    (
        Arc::new(CompiledAnimTreeDefinition::from_nodes(
            nodes,
            0,
            names.len(),
        )),
        Arc::new(script),
    )
}
