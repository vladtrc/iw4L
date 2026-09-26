use crate::Score;

pub const PARSE_SCORES_CAP: usize = 0x12;

pub(crate) const SCORE_TOKENS_PER_CLIENT: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParsedScores {
    pub num: usize,

    pub team_scores_axis: i32,

    pub team_scores_allies: i32,

    pub score_limit: i32,

    pub scores: [Score; PARSE_SCORES_CAP],

    pub gap_0x20: [i32; PARSE_SCORES_CAP],

    pub status_icon_index: [i32; PARSE_SCORES_CAP],
}

impl Default for ParsedScores {
    fn default() -> Self {
        Self {
            num: 0,
            team_scores_axis: 0,
            team_scores_allies: 0,
            score_limit: 0,
            scores: [Score::default(); PARSE_SCORES_CAP],
            gap_0x20: [0; PARSE_SCORES_CAP],
            status_icon_index: [0; PARSE_SCORES_CAP],
        }
    }
}

fn argv<'a>(tokens: &[&'a str], i: usize) -> &'a str {
    tokens.get(i).copied().unwrap_or("")
}

fn atol(s: &str) -> i32 {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_whitespace() {
        i += 1;
    }
    if i >= bytes.len() {
        return 0;
    }
    let neg = match bytes[i] {
        b'-' => {
            i += 1;
            true
        }
        b'+' => {
            i += 1;
            false
        }
        _ => false,
    };
    if i >= bytes.len() || !bytes[i].is_ascii_digit() {
        return 0;
    }
    let mut n: i32 = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        n = n
            .saturating_mul(10)
            .saturating_add(i32::from(bytes[i] - b'0'));
        i += 1;
    }
    if neg { n.saturating_neg() } else { n }
}

pub fn parse_scores(argv_tokens: &[&str]) -> ParsedScores {
    let tag = argv(argv_tokens, 0);
    if tag.as_bytes().first().copied() != Some(b'b') {
        return ParsedScores::default();
    }
    let mut out = ParsedScores::default();
    let mut num = atol(argv(argv_tokens, 1));
    if num > PARSE_SCORES_CAP as i32 {
        num = PARSE_SCORES_CAP as i32;
    }
    if num < 0 {
        num = 0;
    }
    out.num = num as usize;
    out.team_scores_axis = atol(argv(argv_tokens, 2));
    out.team_scores_allies = atol(argv(argv_tokens, 3));
    out.score_limit = atol(argv(argv_tokens, 4));
    for i in 0..out.num {
        let base = 5 + i * SCORE_TOKENS_PER_CLIENT;
        let mut row = Score::default();
        row.client = atol(argv(argv_tokens, base));
        row.score = atol(argv(argv_tokens, base + 1));
        row.ping = atol(argv(argv_tokens, base + 2));
        row.deaths = atol(argv(argv_tokens, base + 3));
        out.status_icon_index[i] = atol(argv(argv_tokens, base + 4));
        row.kills = atol(argv(argv_tokens, base + 5));
        row.assists = atol(argv(argv_tokens, base + 6));
        out.gap_0x20[i] = atol(argv(argv_tokens, base + 7));
        out.scores[i] = row;
    }
    out
}
