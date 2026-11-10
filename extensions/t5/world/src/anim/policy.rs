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
pub const POLICY: AnimationFamily = AnimationFamily {
    grammar: &GRAMMAR,
    select_referenced_leaves: true,
    use_type_aliases: false,
    decode_source: asset_anim::decode_standard_source,
};
