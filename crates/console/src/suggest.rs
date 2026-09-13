pub const SUGGEST_WINDOW: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestTone {
    Indent,

    Matched,

    RestSelected,

    RestMuted,

    Ellipsis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuggestSpan {
    pub text: String,
    pub tone: SuggestTone,
}

pub fn suggestion_spans(
    suggestions: &[String],
    selected: usize,
    token: &str,
    indent_cols: usize,
) -> Vec<SuggestSpan> {
    if suggestions.is_empty() {
        return Vec::new();
    }
    let selected = selected % suggestions.len();
    let end = (selected + SUGGEST_WINDOW).min(suggestions.len());
    let indent = " ".repeat(indent_cols);

    let mut spans = Vec::new();
    if end < suggestions.len() {
        spans.push(SuggestSpan {
            text: format!("{indent}…\n"),
            tone: SuggestTone::Ellipsis,
        });
    }

    for index in (selected..end).rev() {
        let value = &suggestions[index];
        let is_selected = index == selected;
        if !indent.is_empty() {
            spans.push(SuggestSpan {
                text: indent.clone(),
                tone: SuggestTone::Indent,
            });
        }
        push_value_spans(&mut spans, value, token, is_selected);
        if index != selected {
            spans.push(SuggestSpan {
                text: "\n".to_owned(),
                tone: SuggestTone::Indent,
            });
        }
    }
    spans
}

fn push_value_spans(spans: &mut Vec<SuggestSpan>, value: &str, token: &str, selected: bool) {
    let (matched, rest) = split_match(value, token);
    if !matched.is_empty() {
        spans.push(SuggestSpan {
            text: matched,
            tone: SuggestTone::Matched,
        });
    }
    if !rest.is_empty() {
        spans.push(SuggestSpan {
            text: rest,
            tone: if selected {
                SuggestTone::RestSelected
            } else {
                SuggestTone::RestMuted
            },
        });
    }
}

fn split_match(value: &str, token: &str) -> (String, String) {
    if token.is_empty() {
        return (String::new(), value.to_owned());
    }
    let value_lower = value.to_ascii_lowercase();
    let token_lower = token.to_ascii_lowercase();
    if value_lower.starts_with(&token_lower) {
        let split = token.chars().count();
        let matched: String = value.chars().take(split).collect();
        let rest: String = value.chars().skip(split).collect();
        return (matched, rest);
    }
    (String::new(), value.to_owned())
}
