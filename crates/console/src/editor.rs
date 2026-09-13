#[derive(Debug, Default, Clone)]
pub struct ConsoleEditor {
    pub line: String,

    pub caret: usize,
    pub selected: usize,
}

impl ConsoleEditor {
    pub fn insert(&mut self, text: &str) {
        let byte = char_byte(&self.line, self.caret);
        self.line.insert_str(byte, text);
        self.caret += text.chars().count();
        self.selected = 0;
    }

    pub fn kill_word(&mut self) {
        if self.caret == 0 {
            return;
        }
        let chars: Vec<char> = self.line.chars().collect();
        let mut end = self.caret;
        while end > 0 && chars[end - 1].is_whitespace() {
            end -= 1;
        }
        let mut start = end;
        while start > 0 && !chars[start - 1].is_whitespace() {
            start -= 1;
        }
        let a = char_byte(&self.line, start);
        let b = char_byte(&self.line, self.caret);
        self.line.replace_range(a..b, "");
        self.caret = start;
        self.selected = 0;
    }

    pub fn token_at_caret(&self) -> (usize, usize, String) {
        let (start, end) = token_bounds(&self.line, self.caret);
        let text = self.line.chars().skip(start).take(end - start).collect();
        (start, end, text)
    }

    pub fn arg_index(&self) -> usize {
        let chars: Vec<char> = self.line.chars().collect();
        let caret = self.caret.min(chars.len());
        let before: String = chars[..caret].iter().collect();
        let trailing_space = before.ends_with(|c: char| c.is_whitespace());
        let tokens = before.split_whitespace().count();
        if tokens == 0 {
            0
        } else if trailing_space {
            tokens
        } else {
            tokens - 1
        }
    }

    pub fn accept(&mut self, suggestions: &[String]) -> bool {
        if suggestions.is_empty() {
            return false;
        }
        let value = &suggestions[self.selected % suggestions.len()];
        let (start, end) = token_bounds(&self.line, self.caret);
        let a = char_byte(&self.line, start);
        let b = char_byte(&self.line, end);
        self.line.replace_range(a..b, value);
        self.caret = start + value.chars().count();
        if self.line.chars().nth(self.caret) != Some(' ') {
            self.insert(" ");
        }
        self.selected = 0;
        true
    }

    pub fn take(&mut self) -> String {
        self.caret = 0;
        self.selected = 0;
        std::mem::take(&mut self.line)
    }
}

pub fn char_byte(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map_or(value.len(), |(index, _)| index)
}

fn token_bounds(value: &str, caret: usize) -> (usize, usize) {
    let chars: Vec<char> = value.chars().collect();
    let mut start = caret.min(chars.len());
    while start > 0 && !chars[start - 1].is_whitespace() {
        start -= 1;
    }
    let mut end = caret.min(chars.len());
    while end < chars.len() && !chars[end].is_whitespace() {
        end += 1;
    }
    (start, end)
}
