use std::sync::Arc;

use std::collections::HashMap;

use anim_iw4::{
    ANIM_BODY_PART_NAMES, ANIM_COND_IS_BITFLAGS, ANIM_COND_NAMES, ANIM_ET_NAMES, ANIM_MT_NAMES,
    ANIM_PARSE_MODES, anim_cond_evaluable, anim_cond_null_value_defaults_to_one,
    anim_cond_value_names,
};

use crate::animtree::CompiledAnimTreeDefinition;
use crate::atr_compile::AtrParser;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedAnimCommand {
    pub body_part: u8,
    pub anim_index: u16,

    pub duration_ms: Option<i32>,

    pub blend_ms: Option<i32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedAnimCondition {
    pub index: u8,
    pub bitflags: bool,
    pub bits: u64,
    pub value: i32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedAnimItem {
    pub skip: bool,

    pub conditions: Vec<ParsedAnimCondition>,

    pub raw: String,
    pub commands: Vec<ParsedAnimCommand>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedPlayerAnimScript {
    pub slots: Vec<(u8, u8, Vec<ParsedAnimItem>)>,

    pub events: Vec<(u8, Vec<ParsedAnimItem>)>,
    pub item_count: usize,
    pub skipped_items: usize,
    pub command_count: usize,
    pub unresolved_anims: usize,
    pub event_item_count: usize,
}

pub use xmodel_runtime::PlayerAnimProperties;

impl ParsedPlayerAnimScript {
    pub fn animation_properties(&self, index: u16) -> PlayerAnimProperties {
        let mut out = PlayerAnimProperties::default();
        for (_, movetype, items) in &self.slots {
            for command in items
                .iter()
                .flat_map(|item| &item.commands)
                .filter(|c| c.anim_index == index)
            {
                if command.body_part != 2 {
                    out.ladder |= matches!(*movetype, 18 | 19);
                    out.stationary |= (22..32).contains(movetype);
                }
                if let Some(ms) = command.blend_ms {
                    out.blend_ms = ms;
                }
            }
        }
        for (event, items) in &self.events {
            for command in items
                .iter()
                .flat_map(|item| &item.commands)
                .filter(|c| c.anim_index == index)
            {
                out.stationary |= matches!(*event, 1 | 10);
                if *event == 2 {
                    out.blend_ms = 30;
                }
                if let Some(ms) = command.blend_ms {
                    out.blend_ms = ms;
                }
            }
        }
        out
    }

    pub fn slot(&self, state: u8, movetype: u8) -> Option<&[ParsedAnimItem]> {
        self.slots
            .iter()
            .find(|(s, m, _)| *s == state && *m == movetype)
            .map(|(_, _, items)| items.as_slice())
    }

    pub fn event(&self, event: u8) -> Option<&[ParsedAnimItem]> {
        self.events
            .iter()
            .find(|(e, _)| *e == event)
            .map(|(_, items)| items.as_slice())
    }

    pub fn report_line(&self) -> String {
        format!(
            "playeranim.script parsed: slots={} items={} skipped={} commands={} unresolved={} events={} event_items={}",
            self.slots.len(),
            self.item_count,
            self.skipped_items,
            self.command_count,
            self.unresolved_anims,
            self.events.len(),
            self.event_item_count
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerAnimParseError {
    BadToken { offset: usize, message: String },
}

impl core::fmt::Display for PlayerAnimParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadToken { offset, message } => write!(f, "{message} at byte {offset}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownDefine {
    Refuse,
    SkipLine,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownCondition {
    Refuse,
    SkipItem,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedAnimCondition {
    pub name: &'static str,
    pub value: &'static str,
    pub conditions: &'static [ParsedAnimCondition],
}
#[derive(Clone, Copy, Debug)]
pub struct PlayerAnimGrammar {
    pub numeric_modifiers: &'static [&'static str],
    pub flag_modifiers: &'static [&'static str],
    pub unknown_defines: UnknownDefine,
    pub unknown_conditions: UnknownCondition,
    pub named_conditions: &'static [NamedAnimCondition],
}
pub const REFERENCE_ANIM_GRAMMAR: PlayerAnimGrammar = PlayerAnimGrammar {
    numeric_modifiers: &[],
    flag_modifiers: &[],
    unknown_defines: UnknownDefine::Refuse,
    unknown_conditions: UnknownCondition::Refuse,
    named_conditions: &[],
};

pub(crate) fn parse_player_anim_script(
    script: &[u8],
    tree: &CompiledAnimTreeDefinition,
    grammar: &PlayerAnimGrammar,
    player_anim_types: Option<&[u8]>,
) -> Result<Arc<ParsedPlayerAnimScript>, PlayerAnimParseError> {
    let mut parser = AtrParser::new(script);
    let mut parsed = ParsedPlayerAnimScript {
        slots: Vec::new(),
        events: Vec::new(),
        item_count: 0,
        skipped_items: 0,
        command_count: 0,
        unresolved_anims: 0,
        event_item_count: 0,
    };
    let mut mode = 0i32;
    let mut indent = 0u8;
    let mut state = 0u8;
    let mut movetype = 0u8;
    let mut event = 0u8;
    let mut current_items: Option<Vec<ParsedAnimItem>> = None;
    let mut current_event_items: Option<Vec<ParsedAnimItem>> = None;
    let mut aliases: HashMap<(u8, String), u64> = HashMap::new();
    if let Some(types) = player_anim_types {
        let mut types = AtrParser::new(types);
        for index in 0..64 {
            let name = types.parse(true);
            if name.is_empty() {
                break;
            }
            if index_ci(&name, anim_iw4::ANIM_PLAYERANIMTYPE_NAMES).is_none() {
                continue;
            }
            for condition in [
                anim_iw4::ANIM_COND_PLAYERANIMTYPE,
                anim_iw4::ANIM_COND_PLAYERANIMTYPEPRIMARY,
            ] {
                aliases.insert((condition, ascii_lower(&name)), 1u64 << index);
            }
        }
    }

    loop {
        let token = parser.parse(true);
        if token.is_empty() {
            break;
        }
        if let Some(new_mode) = index_ci(&token, ANIM_PARSE_MODES) {
            flush_anim_slot(&mut parsed, &mut current_items, state, movetype);
            flush_event_slot(&mut parsed, &mut current_event_items, event);
            mode = new_mode as i32;
            indent = 0;
            continue;
        }
        if mode == 0 {
            if token.eq_ignore_ascii_case("set") {
                parse_define(&mut parser, &mut aliases, grammar)?;
            }
            continue;
        }
        if mode != 1 && mode != 4 {
            continue;
        }
        if token == "{" {
            if indent >= 3 {
                return Err(bad(&parser, "unexpected '{'"));
            }
            indent += 1;
            continue;
        }
        if token == "}" {
            if indent == 0 {
                return Err(bad(&parser, "unexpected '}'"));
            }
            indent -= 1;
            if mode == 1 && indent == 1 {
                flush_anim_slot(&mut parsed, &mut current_items, state, movetype);
            }
            if mode == 4 && indent == 0 {
                flush_event_slot(&mut parsed, &mut current_event_items, event);
            }
            continue;
        }
        if mode == 4 {
            match indent {
                0 => {
                    flush_event_slot(&mut parsed, &mut current_event_items, event);
                    let Some(et) = index_ci(&token, ANIM_ET_NAMES) else {
                        skip_unknown_block(&mut parser);
                        continue;
                    };
                    event = et as u8;
                    current_event_items = Some(Vec::new());
                }
                1 => {
                    parser.unget();
                    let item = parse_item(&mut parser, tree, &mut parsed, &aliases, grammar)?;
                    if let Some(items) = current_event_items.as_mut() {
                        if items.len() >= 128 {
                            return Err(bad(&parser, "exceeded maximum items per script (128)"));
                        }
                        parsed.event_item_count += 1;
                        if item.skip {
                            parsed.skipped_items += 1;
                        }
                        parsed.command_count += item.commands.len();
                        items.push(item);
                    }
                }
                _ => return Err(bad(&parser, "unexpected token")),
            }
            continue;
        }
        match indent {
            0 => {
                if !token.eq_ignore_ascii_case("state") {
                    return Err(bad(&parser, "expected 'state'"));
                }
                let name = parser.parse(false);
                if name.is_empty() {
                    return Err(bad(&parser, "expected state type"));
                }
                state = index_ci(&name, anim_iw4::ANIM_STATE_NAMES).unwrap_or(0) as u8;
            }
            1 => {
                flush_anim_slot(&mut parsed, &mut current_items, state, movetype);
                let Some(mt) = index_ci(&token, ANIM_MT_NAMES) else {
                    skip_unknown_block(&mut parser);
                    continue;
                };
                movetype = mt as u8;
                current_items = Some(Vec::new());
            }
            2 => {
                parser.unget();
                let item = parse_item(&mut parser, tree, &mut parsed, &aliases, grammar)?;
                if let Some(items) = current_items.as_mut() {
                    if items.len() >= 128 {
                        return Err(bad(&parser, "exceeded maximum items per script (128)"));
                    }
                    parsed.item_count += 1;
                    if item.skip {
                        parsed.skipped_items += 1;
                    }
                    parsed.command_count += item.commands.len();
                    items.push(item);
                }
            }
            _ => return Err(bad(&parser, "unexpected token")),
        }
    }
    flush_anim_slot(&mut parsed, &mut current_items, state, movetype);
    flush_event_slot(&mut parsed, &mut current_event_items, event);
    Ok(Arc::new(parsed))
}

fn flush_anim_slot(
    parsed: &mut ParsedPlayerAnimScript,
    current: &mut Option<Vec<ParsedAnimItem>>,
    state: u8,
    movetype: u8,
) {
    if let Some(items) = current.take() {
        parsed.slots.push((state, movetype, items));
    }
}

fn flush_event_slot(
    parsed: &mut ParsedPlayerAnimScript,
    current: &mut Option<Vec<ParsedAnimItem>>,
    event: u8,
) {
    if let Some(items) = current.take() {
        parsed.events.push((event, items));
    }
}

fn parse_item(
    parser: &mut AtrParser<'_>,
    tree: &CompiledAnimTreeDefinition,
    parsed: &mut ParsedPlayerAnimScript,
    aliases: &HashMap<(u8, String), u64>,
    grammar: &PlayerAnimGrammar,
) -> Result<ParsedAnimItem, PlayerAnimParseError> {
    let (skip, conditions, raw) = parse_conditions(parser, aliases, grammar)?;
    let brace = parser.parse(true);
    if brace != "{" {
        parser.unget();
        return Err(bad(parser, "expected '{' after conditions"));
    }
    let mut commands = Vec::new();
    loop {
        let token = parser.parse(true);
        if token.is_empty() {
            return Err(bad(parser, "unterminated command list"));
        }
        if token == "}" {
            break;
        }
        let Some(body_part) = index_ci(&token, ANIM_BODY_PART_NAMES).filter(|i| *i > 0) else {
            parser.unget();
            break;
        };
        let anim = parser.parse(false);
        if anim.is_empty() {
            return Err(bad(parser, "expected animation"));
        }
        let mut duration_ms = None;
        let mut blend_ms = None;
        loop {
            let extra = parser.parse(false);
            if extra.is_empty() {
                break;
            }
            if extra.eq_ignore_ascii_case("duration") {
                let value = parser.parse(false);
                duration_ms = value.parse().ok();
            } else if extra.eq_ignore_ascii_case("blendtime") {
                blend_ms = Some(
                    parser
                        .parse(false)
                        .parse()
                        .map_err(|_| bad(parser, "expected blendtime value"))?,
                );
            } else if extra.eq_ignore_ascii_case("sound") {
                let _ = parser.parse(false);
            } else if grammar
                .numeric_modifiers
                .iter()
                .any(|name| extra.eq_ignore_ascii_case(name))
            {
                let value = parser.parse(false).parse::<f32>();
                if value.is_err() || value.is_ok_and(|value| !value.is_finite()) {
                    return Err(bad(parser, "expected animation modifier value"));
                }
            } else if extra.eq_ignore_ascii_case("turretanim")
                || grammar
                    .flag_modifiers
                    .iter()
                    .any(|name| extra.eq_ignore_ascii_case(name))
            {
                continue;
            } else {
                parser.unget();
                break;
            }
        }
        match tree.index_of(&ascii_lower(&anim)) {
            Some(anim_index) => {
                if commands.len() >= 12 {
                    return Err(bad(parser, "exceeded maximum number of animations (12)"));
                }
                commands.push(ParsedAnimCommand {
                    body_part: body_part as u8,
                    anim_index,
                    duration_ms,
                    blend_ms,
                });
            }
            None if !skip => parsed.unresolved_anims += 1,
            None => {}
        }
    }
    Ok(ParsedAnimItem {
        skip,
        conditions,
        raw,
        commands,
    })
}

fn parse_define(
    parser: &mut AtrParser<'_>,
    aliases: &mut HashMap<(u8, String), u64>,
    grammar: &PlayerAnimGrammar,
) -> Result<(), PlayerAnimParseError> {
    let cond = parser.parse(false);
    let Some(index) = index_ci(&cond, ANIM_COND_NAMES) else {
        if grammar.unknown_defines == UnknownDefine::SkipLine {
            while !parser.parse(false).is_empty() {}
            return Ok(());
        }
        return Err(bad(parser, "unknown define condition"));
    };
    let alias = ascii_lower(&parser.parse(false));
    let eq = parser.parse(false);
    if eq != "=" {
        parser.unget();
        return Err(bad(parser, "expected '=' in define"));
    }
    let mut bits = 0u64;
    let mut subtract = false;
    loop {
        let token = parser.parse(false);
        if token.is_empty() {
            break;
        }
        if token.eq_ignore_ascii_case("set") || index_ci(&token, ANIM_PARSE_MODES).is_some() {
            parser.unget();
            break;
        }
        if token.eq_ignore_ascii_case("AND") {
            continue;
        }
        if token.eq_ignore_ascii_case("NOT") || token.eq_ignore_ascii_case("MINUS") {
            subtract = true;
            if bits == 0 {
                bits = u64::MAX;
            }
            continue;
        }
        if let Some(value) = resolve_cond_value(index as u8, &token, aliases) {
            let more = value.bits();
            if subtract {
                bits &= !more;
            } else {
                bits |= more;
            }
        }
    }
    aliases.insert((index as u8, alias), bits);
    Ok(())
}

enum ResolvedCondValue {
    Bit(u8),
    Bits(u64),
}

impl ResolvedCondValue {
    fn bits(&self) -> u64 {
        match *self {
            Self::Bit(bit) => 1u64 << bit,
            Self::Bits(bits) => bits,
        }
    }
}

fn resolve_cond_value(
    index: u8,
    token: &str,
    aliases: &HashMap<(u8, String), u64>,
) -> Option<ResolvedCondValue> {
    if ANIM_COND_IS_BITFLAGS[usize::from(index)] && token.eq_ignore_ascii_case("all") {
        return Some(ResolvedCondValue::Bits(u64::MAX));
    }
    if matches!(
        index,
        anim_iw4::ANIM_COND_PLAYERANIMTYPE | anim_iw4::ANIM_COND_PLAYERANIMTYPEPRIMARY
    ) && let Some(bits) = aliases.get(&(index, ascii_lower(token)))
    {
        return Some(ResolvedCondValue::Bits(*bits));
    }
    let names = anim_cond_value_names(index);
    if let Some(v) = index_ci(token, names) {
        return Some(ResolvedCondValue::Bit(v as u8));
    }
    aliases
        .get(&(index, ascii_lower(token)))
        .copied()
        .map(ResolvedCondValue::Bits)
}

fn parse_conditions(
    parser: &mut AtrParser<'_>,
    aliases: &HashMap<(u8, String), u64>,
    grammar: &PlayerAnimGrammar,
) -> Result<(bool, Vec<ParsedAnimCondition>, String), PlayerAnimParseError> {
    let mut skip = false;
    let mut saw = false;
    let mut conditions = Vec::new();
    let mut raw = Vec::new();
    let mut current: Option<ParsedAnimCondition> = None;
    let mut subtract = false;
    loop {
        let token = parser.parse(false);
        if token.is_empty() {
            break;
        }
        if token == "{" {
            parser.unget();
            break;
        }
        if token == "," {
            raw.push(",".to_owned());
            if let Some(cond) = current.take() {
                conditions.push(cond);
            }
            subtract = false;
            continue;
        }
        raw.push(token.clone());
        if token.eq_ignore_ascii_case("default") {
            saw = true;
            continue;
        }
        if grammar
            .named_conditions
            .iter()
            .any(|rule| token.eq_ignore_ascii_case(rule.name))
        {
            if let Some(cond) = current.take() {
                conditions.push(cond);
            }
            let value = parser.parse(false);
            raw.push(value.clone());
            saw = true;
            if let Some(rule) = grammar.named_conditions.iter().find(|rule| {
                token.eq_ignore_ascii_case(rule.name) && value.eq_ignore_ascii_case(rule.value)
            }) {
                conditions.extend_from_slice(rule.conditions);
            } else {
                skip = true;
            }
            continue;
        }
        if let Some(cond) = current.as_mut()
            && let Some(value) = resolve_cond_value(cond.index, &token, aliases)
        {
            if cond.bitflags {
                if subtract {
                    cond.bits &= !value.bits();
                } else {
                    cond.bits |= value.bits();
                }
            } else {
                match value {
                    ResolvedCondValue::Bit(bit) => cond.value = i32::from(bit),
                    ResolvedCondValue::Bits(bits) => {
                        cond.bits = bits;
                        cond.bitflags = true;
                    }
                }
            }
            continue;
        }
        if let Some(index) = index_ci(&token, ANIM_COND_NAMES) {
            if let Some(cond) = current.take() {
                conditions.push(cond);
            }
            saw = true;
            if !anim_cond_evaluable(index as u8) {
                skip = true;
            }
            let value = if anim_cond_null_value_defaults_to_one(index as u8) {
                1
            } else {
                0
            };
            current = Some(ParsedAnimCondition {
                index: index as u8,
                bitflags: ANIM_COND_IS_BITFLAGS[index],
                bits: 0,
                value,
            });
            subtract = false;
            continue;
        }
        if token.eq_ignore_ascii_case("AND") {
            continue;
        }
        if token.eq_ignore_ascii_case("NOT") || token.eq_ignore_ascii_case("MINUS") {
            if let Some(cond) = current.as_mut().filter(|cond| cond.bitflags) {
                subtract = true;
                if cond.bits == 0 {
                    cond.bits = u64::MAX;
                }
            } else {
                skip = true;
            }
            continue;
        }
        if current.is_some() {
            skip = true;
            continue;
        }
        if grammar.unknown_conditions == UnknownCondition::SkipItem {
            skip = true;
            saw = true;
            continue;
        }
        return Err(bad(parser, "unknown condition token"));
    }
    if let Some(cond) = current.take() {
        conditions.push(cond);
    }
    if !saw {
        return Err(bad(parser, "no conditions found"));
    }
    Ok((skip, conditions, raw.join(" ")))
}

fn skip_unknown_block(parser: &mut AtrParser<'_>) {
    let mut depth = 0i32;
    loop {
        let token = parser.parse(true);
        if token.is_empty() {
            return;
        }
        if token == "{" {
            depth += 1;
        } else if token == "}" {
            depth -= 1;
            if depth <= 0 {
                return;
            }
        } else if depth == 0 && token != "{" {
            continue;
        }
    }
}

fn index_ci(token: &str, table: &[&str]) -> Option<usize> {
    table
        .iter()
        .position(|name| token.eq_ignore_ascii_case(name))
}

fn ascii_lower(token: &str) -> String {
    token.chars().map(|c| c.to_ascii_lowercase()).collect()
}

fn bad(parser: &AtrParser<'_>, message: &'static str) -> PlayerAnimParseError {
    PlayerAnimParseError::BadToken {
        offset: parser.last_offset(),
        message: message.to_owned(),
    }
}
