use asset_world::*;

pub const POLICY: WorldFamily = WorldFamily {
    draw: WorldDrawPolicy {
        family: crate::ID,
        resolve_specular_env: false,
        lightmap_requires_image: false,
        decode_color_at_convert: false,
        sun_sample_size_near: 0.25,
    },
    complete_objective_models: None,
    normalize_entities,
    spawn_points: |text| parse_reference_spawn_points(&super::entities::decode_entity_keys(text)),
};

fn normalize_entities(text: &str) -> String {
    let text = super::entities::decode_entity_keys(text);
    normalize_map_entities(
        &text,
        &EntityRules {
            overtime: Some(OvertimeEntityRule {
                target: "dd_bombzone",
                label: "_c",
                replacement: "dd_overtime_bombzone",
                gameobject: "dd_overtime",
            }),
            gametype_aliases: &[("dem", "dd"), ("tdm", "war")],
            augment: |_, _| {},
        },
    )
}
