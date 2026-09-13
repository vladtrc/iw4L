use std::collections::HashSet;
use std::sync::Arc;

use anim_iw4::{
    ANIM_BODY_PART_NAMES, ANIMFLAG_ADDITIVE, ANIMFLAG_COMPLETE, ANIMFLAG_LOOPSYNC,
    ANIMFLAG_NONLOOPSYNC, ANIMTREE_PROPERTY_NAMES,
};

use crate::animtree::{
    AtrCompileError, CompiledAnimNode, CompiledAnimTreeDefinition, MULTIPLAYER_ANIMTREE_PATH,
    PLAYERANIM_SCRIPT_PATH,
};

const BG_FIND_ANIM_NAMES: &[&str] = &["torso", "legs", "turning"];

pub(crate) fn compile_multiplayer(
    atr: Option<&[u8]>,
    script: Option<&[u8]>,
) -> Result<Arc<CompiledAnimTreeDefinition>, AtrCompileError> {
    let atr = atr.ok_or(AtrCompileError::MissingSource {
        path: MULTIPLAYER_ANIMTREE_PATH,
    })?;
    let script = script.ok_or(AtrCompileError::MissingSource {
        path: PLAYERANIM_SCRIPT_PATH,
    })?;
    let mut names = HashSet::new();
    for name in BG_FIND_ANIM_NAMES {
        names.insert((*name).to_owned());
    }
    harvest_script_anim_names(script, &mut names);
    let script_names = names.len();
    let mut ignored = 0usize;
    let children = parse_atr(atr, &names, &mut ignored)?;
    let definition = flatten_tree(children, ignored, script_names)?;
    Ok(Arc::new(definition))
}

fn harvest_script_anim_names(script: &[u8], names: &mut HashSet<String>) {
    let mut parser = ComParser::new(script);
    loop {
        let token = parser.parse(true);
        if token.is_empty() {
            break;
        }
        if !is_body_part(&token) {
            continue;
        }
        let anim = parser.parse(false);
        if anim.is_empty() || !scr_is_identifier(&anim) {
            continue;
        }
        names.insert(ascii_lower(&anim));
    }
}

fn is_body_part(token: &str) -> bool {
    ANIM_BODY_PART_NAMES
        .iter()
        .skip(1)
        .any(|name| token.eq_ignore_ascii_case(name))
}

fn parse_atr(
    src: &[u8],
    names: &HashSet<String>,
    ignored: &mut usize,
) -> Result<Vec<ParsedAnim>, AtrCompileError> {
    let mut parser = ComParser::new(src);
    let (children, eof) = parse_internal(&mut parser, names, ignored, true, false, false)?;
    if !eof {
        return Err(parser.bad_token("bad token"));
    }
    Ok(children)
}

struct ParsedAnim {
    name: String,
    flags: u16,
    children: Vec<ParsedAnim>,
}

#[allow(clippy::never_loop)]
fn parse_internal(
    parser: &mut ComParser<'_>,
    names: &HashSet<String>,
    ignored: &mut usize,
    include_parent: bool,
    loop_sync: bool,
    complete: bool,
) -> Result<(Vec<ParsedAnim>, bool), AtrCompileError> {
    let mut children = Vec::new();
    let mut current: Option<WorkAnim> = None;

    loop {
        loop {
            loop {
                let token = parser.parse(true);
                if parser.hit_eof && token.is_empty() {
                    finish_parent(&mut children, include_parent, loop_sync);
                    return Ok((children, true));
                }
                if !scr_is_identifier(&token) {
                    break;
                }
                if let Some(work) = current.take() {
                    if !work.ignore {
                        children.push(work.into_leaf());
                    } else {
                        *ignored += 1;
                    }
                }
                let name = ascii_lower(&token);
                if children.iter().any(|child| child.name == name) {
                    return Err(AtrCompileError::DuplicateAnimation { name });
                }
                let ignore = !complete && !names.contains(&name);
                let on_line = parser.parse(false);
                if on_line.is_empty() {
                    current = Some(WorkAnim {
                        name,
                        flags: 0,
                        ignore,
                        children: Vec::new(),
                    });
                    continue;
                }
                if scr_is_identifier(&on_line) {
                    return Err(parser.bad_token("FIXME: aliases not yet implemented"));
                }
                if on_line != ":" {
                    return Err(parser.bad_token("bad token"));
                }
                let flags = parse_properties(parser)?;
                let brace = parser.parse(true);
                if brace != "{" {
                    return Err(
                        parser.bad_token("properties cannot be applied to primitive animations")
                    );
                }
                current = Some(WorkAnim {
                    name,
                    flags,
                    ignore,
                    children: Vec::new(),
                });
                break;
            }
            let token = parser.last_token();
            if token != "{" {
                break;
            }
            let after = parser.parse(false);
            if !after.is_empty() {
                return Err(parser.bad_token("token not allowed after '{'"));
            }
            let Some(mut work) = current.take() else {
                return Err(parser.bad_token("no animation specified for this block"));
            };
            let child_complete =
                complete || ((work.flags & ANIMFLAG_COMPLETE) != 0 && !work.ignore);
            let (nested, nested_eof) = parse_internal(
                parser,
                names,
                ignored,
                !work.ignore,
                (work.flags & ANIMFLAG_LOOPSYNC) != 0,
                child_complete,
            )?;
            if nested_eof {
                return Err(parser.bad_token("unexpected end of file"));
            }
            if nested.is_empty() {
                *ignored += 1;
            } else {
                work.children = nested;
                if work.ignore {
                    *ignored += 1;
                }

                children.push(work.into_node());
            }
        }
        let token = parser.last_token();
        if token == "}" {
            let after = parser.parse(false);
            if !after.is_empty() {
                return Err(parser.bad_token("token not allowed after '}'"));
            }
            if let Some(work) = current.take() {
                if work.ignore {
                    *ignored += 1;
                } else {
                    children.push(work.into_leaf());
                }
            }
            finish_parent(&mut children, include_parent, loop_sync);
            return Ok((children, false));
        }
        return Err(parser.bad_token("bad token"));
    }
}

struct WorkAnim {
    name: String,
    flags: u16,
    ignore: bool,
    children: Vec<ParsedAnim>,
}

impl WorkAnim {
    fn into_leaf(self) -> ParsedAnim {
        ParsedAnim {
            name: self.name,
            flags: 0,
            children: Vec::new(),
        }
    }

    fn into_node(self) -> ParsedAnim {
        ParsedAnim {
            name: self.name,
            flags: self.flags,
            children: self.children,
        }
    }
}

fn finish_parent(children: &mut Vec<ParsedAnim>, include_parent: bool, loop_sync: bool) {
    if include_parent && children.is_empty() {
        children.push(ParsedAnim {
            name: if loop_sync {
                "void_loop".to_owned()
            } else {
                "void".to_owned()
            },
            flags: 0,
            children: Vec::new(),
        });
    }
}

fn parse_properties(parser: &mut ComParser<'_>) -> Result<u16, AtrCompileError> {
    let mut flags = 0u16;
    loop {
        let token = parser.parse(false);
        if token.is_empty() {
            return Ok(flags);
        }
        let index = ANIMTREE_PROPERTY_NAMES
            .iter()
            .position(|name| token.eq_ignore_ascii_case(name));
        match index {
            Some(0) => flags |= ANIMFLAG_LOOPSYNC,
            Some(1) => flags |= ANIMFLAG_NONLOOPSYNC,
            Some(2) => flags |= ANIMFLAG_COMPLETE,
            Some(3) => flags |= ANIMFLAG_ADDITIVE,
            _ => return Err(parser.bad_token("unknown anim property")),
        }
    }
}

fn flatten_tree(
    children: Vec<ParsedAnim>,
    ignored: usize,
    script_names: usize,
) -> Result<CompiledAnimTreeDefinition, AtrCompileError> {
    let size = anim_tree_size(&children);
    if size == 0 {
        return Err(AtrCompileError::EmptyTree);
    }
    let mut nodes = vec![CompiledAnimNode::blank(); size];
    let next = create_animation_tree(&mut nodes, &children, 1, "root", 0, 0);
    if next != size {
        return Err(AtrCompileError::BadToken {
            offset: 0,
            message: format!("packed size {next} != tree size {size}"),
        });
    }
    Ok(CompiledAnimTreeDefinition::from_nodes(
        nodes,
        ignored,
        script_names,
    ))
}

fn anim_tree_size(children: &[ParsedAnim]) -> usize {
    let mut size = 0usize;
    for child in children {
        if child.children.is_empty() {
            size += 1;
        } else {
            size += anim_tree_size(&child.children);
        }
    }
    if size != 0 {
        size += 1;
    }
    size
}

fn create_animation_tree(
    nodes: &mut [CompiledAnimNode],
    siblings: &[ParsedAnim],
    child_index: usize,
    parent_name: &str,
    parent_index: usize,
    flags: u16,
) -> usize {
    let size = siblings.len();
    nodes[parent_index] = CompiledAnimNode {
        name: parent_name.to_owned(),
        parent: nodes[parent_index].parent,
        flags,
        child_count: size as u16,
        first_child: child_index as u16,
    };
    for i in 0..size {
        nodes[child_index + i].parent = Some(parent_index as u16);
    }
    let mut slot = child_index;
    let mut nested = size + child_index;
    for process_additive in 0..=1 {
        for sibling in siblings {
            let additive = (sibling.flags & ANIMFLAG_ADDITIVE) != 0;
            if sibling.children.is_empty() {
                if process_additive != 0 {
                    continue;
                }
                nodes[slot].name = sibling.name.clone();
                nodes[slot].flags = 0;
                nodes[slot].child_count = 0;
                nodes[slot].first_child = 0;
                slot += 1;
            } else if additive == (process_additive == 1) {
                nested = create_animation_tree(
                    nodes,
                    &sibling.children,
                    nested,
                    &sibling.name,
                    slot,
                    sibling.flags,
                );
                slot += 1;
            }
        }
    }
    nested
}

fn ascii_lower(token: &str) -> String {
    token.chars().map(|c| c.to_ascii_lowercase()).collect()
}

fn scr_is_identifier(token: &str) -> bool {
    !token.is_empty() && token.bytes().all(is_c_sym)
}

fn is_c_sym(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

pub(crate) struct ComParser<'a> {
    src: &'a [u8],
    pos: usize,
    hit_eof: bool,
    last: String,
    last_offset: usize,
}

impl<'a> ComParser<'a> {
    pub(crate) fn new(src: &'a [u8]) -> Self {
        Self {
            src,
            pos: 0,
            hit_eof: false,
            last: String::new(),
            last_offset: 0,
        }
    }

    pub(crate) fn unget(&mut self) {
        if self.last_offset <= self.src.len() {
            self.pos = self.last_offset;
            self.hit_eof = false;
        }
    }

    fn last_token(&self) -> &str {
        &self.last
    }

    pub(crate) fn last_offset(&self) -> usize {
        self.last_offset
    }

    fn bad_token(&self, message: &'static str) -> AtrCompileError {
        AtrCompileError::BadToken {
            offset: self.last_offset,
            message: message.to_owned(),
        }
    }

    pub(crate) fn parse(&mut self, allow_line_breaks: bool) -> String {
        self.last.clear();
        if self.hit_eof {
            return String::new();
        }
        let Some((token, next, offset)) = parse_ext(self.src, self.pos, allow_line_breaks) else {
            self.hit_eof = true;
            self.last_offset = self.src.len();
            return String::new();
        };
        if token.is_empty() {
            self.last_offset = offset;
            return String::new();
        }
        self.pos = next;
        self.last_offset = offset;
        self.last.push_str(token);
        self.last.clone()
    }
}

fn parse_ext(src: &[u8], start: usize, allow_line_breaks: bool) -> Option<(&str, usize, usize)> {
    let (data, has_newlines) = skip_whitespace(src, start)?;
    if has_newlines && !allow_line_breaks {
        return Some(("", start, start));
    }
    let c = src[data];
    if c == b'"' {
        return parse_quoted(src, data);
    }
    if c.is_ascii_digit()
        || (c == b'.' && src.get(data + 1).is_some_and(|n| n.is_ascii_digit()))
        || (c == b'-' && src.get(data + 1).is_some_and(|n| n.is_ascii_digit()))
    {
        return parse_number(src, data);
    }
    if is_ident_start(c) {
        return parse_ident(src, data);
    }
    for punct in [
        "+=", "-=", "*=", "/=", "&=", "|=", "++", "--", "&&", "||", "<=", ">=", "==", "!=",
    ] {
        let bytes = punct.as_bytes();
        if src[data..].starts_with(bytes) {
            let end = data + bytes.len();
            return Some((punct, end, data));
        }
    }
    let end = data + 1;
    Some((core::str::from_utf8(&src[data..end]).ok()?, end, data))
}

fn skip_whitespace(src: &[u8], mut i: usize) -> Option<(usize, bool)> {
    let mut has_newlines = false;
    loop {
        while i < src.len() && src[i] <= b' ' {
            if src[i] == b'\n' {
                has_newlines = true;
            }
            i += 1;
        }
        if i >= src.len() {
            return None;
        }
        if src[i] == b'/' && src.get(i + 1) == Some(&b'/') {
            i += 2;
            while i < src.len() && src[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if src[i] == b'/' && src.get(i + 1) == Some(&b'*') {
            i += 2;
            while i + 1 < src.len() && !(src[i] == b'*' && src[i + 1] == b'/') {
                i += 1;
            }
            if i + 1 < src.len() {
                i += 2;
            }
            continue;
        }
        return Some((i, has_newlines));
    }
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b'/' || c == b'\\'
}

fn parse_ident(src: &[u8], start: usize) -> Option<(&str, usize, usize)> {
    let mut i = start + 1;
    while i < src.len() {
        let c = src[i];
        if c.is_ascii_alphanumeric() || c == b'_' {
            i += 1;
        } else {
            break;
        }
    }
    Some((core::str::from_utf8(&src[start..i]).ok()?, i, start))
}

fn parse_number(src: &[u8], start: usize) -> Option<(&str, usize, usize)> {
    let mut i = start + 1;
    while i < src.len() && (src[i].is_ascii_digit() || src[i] == b'.') {
        i += 1;
    }
    if i < src.len() && (src[i] == b'e' || src[i] == b'E') {
        i += 1;
        if i < src.len() && (src[i] == b'+' || src[i] == b'-') {
            i += 1;
        }
        while i < src.len() && src[i].is_ascii_digit() {
            i += 1;
        }
    }
    Some((core::str::from_utf8(&src[start..i]).ok()?, i, start))
}

fn parse_quoted(src: &[u8], start: usize) -> Option<(&str, usize, usize)> {
    let mut i = start + 1;
    while i < src.len() {
        if src[i] == b'\\' && matches!(src.get(i + 1), Some(&b'"') | Some(&b'\\')) {
            i += 2;
            continue;
        }
        if src[i] == b'"' {
            let end = i + 1;
            return Some((core::str::from_utf8(&src[start + 1..i]).ok()?, end, start));
        }
        i += 1;
    }
    Some((
        core::str::from_utf8(&src[start + 1..]).ok()?,
        src.len(),
        start,
    ))
}
