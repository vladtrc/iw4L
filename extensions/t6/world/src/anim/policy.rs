use asset_anim::{
    AnimationFamily, NamedAnimCondition, ParsedAnimCondition, PlayerAnimGrammar, UnknownCondition,
    UnknownDefine,
};
const GRAMMAR: PlayerAnimGrammar = PlayerAnimGrammar {
    numeric_modifiers: &["blendouttime", "animrate"],
    flag_modifiers: &["grenadeanim", "weapontimescale", "allowmovementinterrupt"],
    unknown_defines: UnknownDefine::SkipLine,
    unknown_conditions: UnknownCondition::SkipItem,
    named_conditions: &[
        NamedAnimCondition {
            name: "stance",
            value: "crouch",
            conditions: &[ParsedAnimCondition {
                index: anim_iw4::ANIM_COND_MOVETYPE,
                bitflags: true,
                bits: 209092,
                value: 0,
            }],
        },
        NamedAnimCondition {
            name: "stance",
            value: "prone",
            conditions: &[ParsedAnimCondition {
                index: anim_iw4::ANIM_COND_MOVETYPE,
                bitflags: true,
                bits: 776,
                value: 0,
            }],
        },
        NamedAnimCondition {
            name: "stance",
            value: "stand",
            conditions: &[ParsedAnimCondition {
                index: anim_iw4::ANIM_COND_MOVETYPE,
                bitflags: true,
                bits: 18446744073709341747,
                value: 0,
            }],
        },
        NamedAnimCondition {
            name: "direction",
            value: "backward",
            conditions: &[
                ParsedAnimCondition {
                    index: anim_iw4::ANIM_COND_MOVETYPE,
                    bitflags: true,
                    bits: 10912,
                    value: 0,
                },
                ParsedAnimCondition {
                    index: anim_iw4::ANIM_COND_STRAFING,
                    bitflags: false,
                    bits: 0,
                    value: 0,
                },
            ],
        },
        NamedAnimCondition {
            name: "direction",
            value: "forward",
            conditions: &[
                ParsedAnimCondition {
                    index: anim_iw4::ANIM_COND_MOVETYPE,
                    bitflags: true,
                    bits: 18446744073709540703,
                    value: 0,
                },
                ParsedAnimCondition {
                    index: anim_iw4::ANIM_COND_STRAFING,
                    bitflags: false,
                    bits: 0,
                    value: 0,
                },
            ],
        },
        NamedAnimCondition {
            name: "direction",
            value: "left",
            conditions: &[ParsedAnimCondition {
                index: anim_iw4::ANIM_COND_STRAFING,
                bitflags: false,
                bits: 0,
                value: 1,
            }],
        },
        NamedAnimCondition {
            name: "direction",
            value: "right",
            conditions: &[ParsedAnimCondition {
                index: anim_iw4::ANIM_COND_STRAFING,
                bitflags: false,
                bits: 0,
                value: 2,
            }],
        },
    ],
};
fn decode_source(name: &str, data: &[u8], zlib_compressed: bool) -> Result<Vec<u8>, String> {
    if !zlib_compressed && name == asset_anim::MULTIPLAYER_ANIMTREE_PATH {
        let size = u32::from_le_bytes(
            data.get(..4)
                .ok_or("animation source missing size prefix")?
                .try_into()
                .unwrap(),
        ) as usize;
        let bytes = asset_transport::inflate_deflate(&data[4..], size)?;
        if bytes.len() != size {
            return Err("animation source decoded size mismatch".to_owned());
        }
        Ok(bytes)
    } else {
        asset_anim::decode_standard_source(name, data, zlib_compressed)
    }
}
pub const POLICY: AnimationFamily = AnimationFamily {
    grammar: &GRAMMAR,
    select_referenced_leaves: true,
    use_type_aliases: true,
    decode_source,
};
