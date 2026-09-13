pub fn normalize_command_paste(text: &str) -> String {
    text.chars()
        .map(|c| if matches!(c, '\r' | '\n') { ' ' } else { c })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptCopy {
    Ignored,

    Copied,

    Failed(String),
}

pub fn copy_prompt_selection(
    selected: Option<&str>,
    write: impl FnOnce(&str) -> Result<(), String>,
) -> PromptCopy {
    let Some(text) = selected.filter(|s| !s.is_empty()) else {
        return PromptCopy::Ignored;
    };
    match write(text) {
        Ok(()) => PromptCopy::Copied,
        Err(error) => PromptCopy::Failed(error),
    }
}

pub fn copy_prompt_echo(result: &PromptCopy) -> Option<String> {
    match result {
        PromptCopy::Ignored => None,
        PromptCopy::Copied => Some("Text copied".to_owned()),
        PromptCopy::Failed(error) => Some(format!("clipboard write failed: {error}")),
    }
}

pub fn map_glyphs_to_chars(text: &str, glyph_n: usize) -> Vec<usize> {
    text.char_indices()
        .filter(|(_, c)| *c != '\n')
        .map(|(byte, _)| text[..byte].chars().count())
        .take(glyph_n)
        .collect()
}

pub type ScrollCell = (usize, usize);

pub type RunBounds = (f32, f32, f32, f32);

pub fn run_line_order(cells: &[ScrollCell]) -> Vec<usize> {
    let mut lines = Vec::new();
    for (line, _) in cells {
        if !lines.contains(line) {
            lines.push(*line);
        }
    }
    lines
}

fn row_chars(cells: &[ScrollCell], lines: &[usize], row: usize) -> Vec<usize> {
    let Some(line) = lines.get(row) else {
        return Vec::new();
    };
    cells
        .iter()
        .filter(|(l, _)| l == line)
        .map(|(_, ch)| *ch)
        .collect()
}

pub fn hit_scroll_cell(
    cells: &[ScrollCell],
    runs: &[RunBounds],
    local: (f32, f32),
) -> Option<(usize, usize)> {
    let lines = run_line_order(cells);
    if lines.is_empty() || runs.is_empty() {
        return None;
    }
    let row = runs
        .iter()
        .position(|&(_, _, _, max_y)| local.1 <= max_y)
        .unwrap_or(runs.len() - 1);
    let (min_x, _, max_x, _) = runs[row];
    let n = row_chars(cells, &lines, row).len();
    let col = if n == 0 {
        0
    } else {
        let adv = (max_x - min_x) / n as f32;
        if adv <= 0.0 {
            0
        } else {
            (((local.0 - min_x) / adv).floor() as isize).clamp(0, n as isize) as usize
        }
    };
    Some((row, col))
}

pub fn cell_char_index(
    cells: &[ScrollCell],
    runs: &[RunBounds],
    row: usize,
    col: usize,
) -> Option<usize> {
    let lines = run_line_order(cells);
    if row >= runs.len() {
        return None;
    }
    let chars = row_chars(cells, &lines, row);
    if chars.is_empty() {
        return None;
    }
    if col < chars.len() {
        Some(chars[col])
    } else if col == chars.len() {
        Some(chars[chars.len() - 1] + 1)
    } else {
        None
    }
}

pub fn line_char_range(
    cells: &[ScrollCell],
    runs: &[RunBounds],
    row: usize,
) -> Option<(usize, usize)> {
    let lines = run_line_order(cells);
    if row >= runs.len() {
        return None;
    }
    let chars = row_chars(cells, &lines, row);
    match (chars.first(), chars.last()) {
        (Some(first), Some(last)) => Some((*first, last + 1)),
        _ => None,
    }
}

pub fn cell_rects_for_char_range(
    lo: usize,
    hi: usize,
    cells: &[ScrollCell],
    runs: &[RunBounds],
    block_right: f32,
) -> Vec<RunBounds> {
    let lines = run_line_order(cells);
    let mut rects = Vec::new();
    for (row, &(min_x, min_y, max_x, max_y)) in runs.iter().enumerate() {
        let chars = row_chars(cells, &lines, row);
        let Some(&first) = chars.first() else {
            continue;
        };
        let last = chars[chars.len() - 1];
        let newline = last + 1;
        if hi <= first || lo > newline {
            continue;
        }
        let n = chars.len();
        let start_col = chars.iter().filter(|c| **c < lo).count().min(n);
        let end_col = chars.iter().filter(|c| **c < hi).count().min(n);
        let through_newline = hi > newline && lo <= newline;
        if start_col >= end_col && !through_newline {
            continue;
        }
        let adv = (max_x - min_x) / n as f32;
        let x0 = min_x + start_col as f32 * adv;
        let x1 = if through_newline {
            block_right
        } else {
            min_x + end_col as f32 * adv
        };
        if x1 > x0 {
            rects.push((x0, min_y, x1, max_y));
        }
    }
    rects
}

pub fn ordered_char_range(a: usize, b: usize) -> (usize, usize) {
    if a <= b { (a, b) } else { (b, a) }
}

pub fn selection_len(a: usize, b: usize) -> usize {
    let (lo, hi) = ordered_char_range(a, b);
    hi.saturating_sub(lo)
}

pub fn slice_char_range(text: &str, a: usize, b: usize) -> &str {
    let (lo, hi) = ordered_char_range(a, b);
    let mut indices = text.char_indices().map(|(i, _)| i);
    let start = indices.nth(lo).unwrap_or(text.len());
    let end = if hi == lo {
        start
    } else {
        text.char_indices()
            .map(|(i, _)| i)
            .nth(hi)
            .unwrap_or(text.len())
    };
    &text[start..end]
}

pub fn word_bounds(text: &str, at: usize) -> (usize, usize) {
    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return (0, 0);
    }
    let at = at.min(chars.len().saturating_sub(1));
    let word = |c: char| !c.is_whitespace();
    let want = word(chars[at]);
    let mut lo = at;
    let mut hi = at + 1;
    while lo > 0 && word(chars[lo - 1]) == want {
        lo -= 1;
    }
    while hi < chars.len() && word(chars[hi]) == want {
        hi += 1;
    }
    (lo, hi)
}
