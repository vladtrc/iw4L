use asset_iw4::{SND_ENTCHANNEL_DEFAULT_MAX_VOICES, SND_ENTCHANNEL_FILE, SND_ENTCHANNEL_MAX};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntChannel {
    pub name: String,
    pub priority: i32,

    pub is_3d: bool,

    pub is_restricted: bool,

    pub is_pausable: bool,

    pub max_voices: i32,
}

pub fn parse_ent_channel_file(src: &str) -> Result<Vec<EntChannel>, String> {
    let mut out = Vec::new();
    for (line_no, raw) in src.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut cols = line.split(',').map(str::trim);
        let Some(name) = cols.next().filter(|s| !s.is_empty()) else {
            continue;
        };
        if name.len() > 0x40 {
            return Err(format!(
                "channel name too long (max chars 64): {name} in file [{}]",
                SND_ENTCHANNEL_FILE
            ));
        }
        if out
            .iter()
            .any(|c: &EntChannel| c.name.eq_ignore_ascii_case(name))
        {
            return Err(format!(
                "duplicate channel name '{name}' in file [{}]",
                SND_ENTCHANNEL_FILE
            ));
        }
        let priority = cols.next().map(parse_priority).unwrap_or(0);
        let is_3d = cols.next().map(parse_is_3d).unwrap_or(false);
        let is_restricted = cols.next().map(parse_restricted).unwrap_or(true);
        let is_pausable = cols.next().map(parse_pausable).unwrap_or(true);
        let max_voices = cols
            .next()
            .map(parse_max_voices)
            .unwrap_or(SND_ENTCHANNEL_DEFAULT_MAX_VOICES);
        out.push(EntChannel {
            name: name.to_owned(),
            priority,
            is_3d,
            is_restricted,
            is_pausable,
            max_voices,
        });
        if out.len() > SND_ENTCHANNEL_MAX {
            return Err(format!(
                "channel definition file exceeded max number of channels ({}). (line {})",
                SND_ENTCHANNEL_MAX,
                line_no + 1
            ));
        }
    }
    Ok(out)
}

fn parse_priority(token: &str) -> i32 {
    if token.is_empty() {
        0
    } else {
        token.parse().unwrap_or(0)
    }
}

fn eq_ignore(token: &str, want: &str) -> bool {
    token.eq_ignore_ascii_case(want)
}

fn parse_is_3d(token: &str) -> bool {
    !token.is_empty() && eq_ignore(token, "3d")
}

fn parse_restricted(token: &str) -> bool {
    if token.is_empty() || eq_ignore(token, "restricted") {
        true
    } else {
        !eq_ignore(token, "unrestricted")
    }
}

fn parse_pausable(token: &str) -> bool {
    if token.is_empty() || eq_ignore(token, "pause") {
        true
    } else {
        !eq_ignore(token, "nopause")
    }
}

fn parse_max_voices(token: &str) -> i32 {
    if token.is_empty() {
        return SND_ENTCHANNEL_DEFAULT_MAX_VOICES;
    }
    match token.parse::<i32>() {
        Ok(n) if n >= 1 && n <= SND_ENTCHANNEL_DEFAULT_MAX_VOICES => n,
        _ => SND_ENTCHANNEL_DEFAULT_MAX_VOICES,
    }
}
