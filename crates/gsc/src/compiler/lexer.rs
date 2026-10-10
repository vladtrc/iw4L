use crate::{Fault, Location};

#[derive(Clone, Debug)]
pub struct Token {
    pub text: String,
    pub string: bool,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub fn is(&self, text: &str) -> bool {
        !self.string && self.text == text
    }

    pub fn assignment_operator(&self) -> Option<&str> {
        if !self.string
            && [
                "=", "+=", "-=", "*=", "/=", "|=", "&=", "^=", "%=", "++", "--",
            ]
            .contains(&self.text.as_str())
        {
            Some(&self.text)
        } else {
            None
        }
    }
}

pub fn lex(module: &str, source: &str) -> Result<Vec<Token>, Fault> {
    let chars: Vec<char> = source.chars().collect();
    let (mut i, mut line, mut column) = (0, 1, 1);
    let mut tokens = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            if c == '\n' {
                line += 1;
                column = 1;
            } else {
                column += 1;
            }
            i += 1;
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'/') {
            while i < chars.len() && chars[i] != '\n' {
                i += 1;
                column += 1;
            }
            continue;
        }
        if c == '/' && chars.get(i + 1) == Some(&'*') {
            let start = Location {
                module: module.into(),
                function: String::new(),
                line,
                column,
            };
            i += 2;
            column += 2;
            while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                if chars[i] == '\n' {
                    line += 1;
                    column = 1;
                } else {
                    column += 1;
                }
                i += 1;
            }
            if i + 1 >= chars.len() {
                return Err(Fault::at(&start, "unterminated comment"));
            }
            i += 2;
            column += 2;
            continue;
        }
        let (start, start_column) = (i, column);
        let mut string = false;
        let text;
        if c == '"' {
            string = true;
            i += 1;
            column += 1;
            let mut value = String::new();
            while i < chars.len() && chars[i] != '"' {
                let mut c = chars[i];
                if c == '\\' {
                    i += 1;
                    column += 1;
                    c = match chars.get(i) {
                        Some('n') => '\n',
                        Some('r') => '\r',
                        Some('t') => '\t',
                        Some('"') => '"',
                        Some('\\') => '\\',
                        Some(c) => *c,
                        _ => {
                            return Err(Fault::at(
                                &Location {
                                    module: module.into(),
                                    function: String::new(),
                                    line,
                                    column,
                                },
                                "unsupported string escape",
                            ));
                        }
                    };
                }
                if chars[i] == '\n' {
                    return Err(Fault::at(
                        &Location {
                            module: module.into(),
                            function: String::new(),
                            line,
                            column,
                        },
                        "newline in string",
                    ));
                }
                value.push(c);
                i += 1;
                column += 1;
            }
            if i == chars.len() {
                return Err(Fault::at(
                    &Location {
                        module: module.into(),
                        function: String::new(),
                        line,
                        column,
                    },
                    "unterminated string",
                ));
            }
            i += 1;
            column += 1;
            text = value;
        } else {
            i += 1;
            if c.is_ascii_alphabetic() || c == '_' {
                while i < chars.len()
                    && (chars[i].is_ascii_alphanumeric() || matches!(chars[i], '_' | '\\'))
                {
                    i += 1;
                }
            } else if c.is_ascii_digit()
                || (c == '.' && chars.get(i).is_some_and(char::is_ascii_digit))
            {
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    i += 1;
                }
            } else if let Some(next) = chars.get(i) {
                let pair = format!("{c}{next}");
                if [
                    "::", "==", "!=", "<=", ">=", "&&", "||", "++", "--", "+=", "-=", "*=", "/=",
                    "|=", "&=", "^=", "%=", "<<", ">>",
                ]
                .contains(&pair.as_str())
                {
                    i += 1;
                }
            }
            text = chars[start..i].iter().collect();
            column += i - start;
        }
        tokens.push(Token {
            text,
            string,
            line,
            column: start_column,
        });
    }
    tokens.push(Token {
        text: String::new(),
        string: false,
        line,
        column,
    });
    let mut production = Vec::new();
    let mut depth = 0usize;
    let mut cursor = 0;
    while cursor < tokens.len() {
        let token = &tokens[cursor];
        let pair = tokens
            .get(cursor + 1)
            .filter(|next| !token.string && !next.string)
            .map(|next| (token.text.as_str(), next.text.as_str()));
        match pair {
            Some(("/", "#")) => {
                depth += 1;
                cursor += 2;
            }
            Some(("#", "/")) if depth > 0 => {
                depth -= 1;
                cursor += 2;
            }
            _ => {
                if depth == 0 {
                    production.push(token.clone());
                }
                cursor += 1;
            }
        }
    }
    if depth != 0 {
        return Err(Fault::at(
            &Location {
                module: module.into(),
                function: String::new(),
                line,
                column,
            },
            "unterminated developer block",
        ));
    }
    Ok(production)
}
