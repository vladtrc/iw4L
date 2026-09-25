use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
struct Token {
    text: String,
    string: bool,
    line: usize,
    column: usize,
}

fn lex(module: &str, source: &str) -> Result<Vec<Token>, Fault> {
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum CallSite {
    Call(Namespace),
    Spawn,
    Reference,
}

struct Pending {
    module: String,
    name: String,
    site: CallSite,
    statement: Option<usize>,
}

#[derive(Default)]
struct Tables {
    symbols: Vec<Arc<str>>,
    symbol_ids: BTreeMap<Arc<str>, u32>,
    pending: Vec<Pending>,
}

struct Parser {
    tables: Tables,
    slots: BTreeMap<String, u32>,
    module: String,
    function: String,
    tokens: Vec<Token>,
    pos: usize,
    code: Vec<(Location, Op)>,
    includes: Vec<String>,
    dependencies: BTreeSet<String>,
    loops: Vec<(Vec<usize>, Option<Vec<usize>>)>,
    constants: BTreeMap<String, Value>,
    animtree: Option<String>,
}
impl Parser {
    fn location(&self) -> Location {
        let t = &self.tokens[self.pos];
        Location {
            module: self.module.clone(),
            function: self.function.clone(),
            line: t.line,
            column: t.column,
        }
    }
    fn error(&self, message: impl Into<String>) -> Fault {
        Fault::at(&self.location(), message)
    }
    fn is(&self, text: &str) -> bool {
        !self.tokens[self.pos].string && self.tokens[self.pos].text == text
    }
    fn eat(&mut self, text: &str) -> bool {
        if self.is(text) {
            self.pos += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, text: &str) -> Result<(), Fault> {
        if self.eat(text) {
            Ok(())
        } else {
            Err(self.error(format!(
                "expected {text:?}, found {:?}",
                self.tokens[self.pos].text
            )))
        }
    }
    fn ident(&mut self) -> Result<String, Fault> {
        let t = &self.tokens[self.pos];
        if t.string
            || !t
                .text
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            return Err(self.error("expected identifier"));
        }
        let text = t.text.clone();
        self.pos += 1;
        Ok(text)
    }
    fn symbol(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.tables.symbol_ids.get(name) {
            return id;
        }
        let id = self.tables.symbols.len() as u32;
        let name: Arc<str> = name.into();
        self.tables.symbols.push(name.clone());
        self.tables.symbol_ids.insert(name, id);
        id
    }
    fn slot(&mut self, name: &str) -> u32 {
        let next = self.slots.len() as u32;
        *self.slots.entry(name.to_owned()).or_insert(next)
    }
    fn unlinked(&mut self, name: String, site: CallSite) -> u32 {
        self.tables.pending.push(Pending {
            module: self.module.clone(),
            name,
            site,
            statement: None,
        });
        (self.tables.pending.len() - 1) as u32
    }
    fn load(&mut self, name: &str) -> Op {
        match name {
            "self" => Op::Global(Global::SelfRef),
            "level" => Op::Global(Global::Level),
            "game" => Op::Global(Global::Game),
            _ => Op::Load(self.slot(name)),
        }
    }
    fn number(&self, text: &str) -> Option<Result<Value, Fault>> {
        let digits = text.strip_prefix('-').unwrap_or(text);
        if !digits.starts_with(|c: char| c.is_ascii_digit() || c == '.')
            || !digits.bytes().any(|b| b.is_ascii_digit())
        {
            return None;
        }
        Some(if text.contains('.') {
            match text.parse::<f32>() {
                Ok(n) if n.is_finite() => Ok(Value::Float(n)),
                _ => Err(self.error("invalid float literal")),
            }
        } else if !digits.bytes().all(|b| b.is_ascii_digit()) {
            Err(self.error("invalid integer literal"))
        } else {
            // Retail's CRT scanf accumulates %d in 32 bits: long literals wrap, never saturate.
            let n = digits.bytes().fold(0u32, |n, b| {
                n.wrapping_mul(10).wrapping_add(u32::from(b - b'0'))
            }) as i32;
            Ok(Value::Int(if text.starts_with('-') {
                n.wrapping_neg()
            } else {
                n
            }))
        })
    }
    fn emit(&mut self, op: Op) -> usize {
        let pc = self.code.len();
        self.code.push((self.location(), op));
        pc
    }
    fn patch(&mut self, pc: usize, target: usize) {
        match &mut self.code[pc].1 {
            Op::Jump(to) | Op::JumpFalse(to) => *to = target,
            _ => unreachable!(),
        }
    }
    fn arguments(&mut self) -> Result<usize, Fault> {
        self.expect("(")?;
        let mut count = 0;
        if !self.is(")") {
            loop {
                self.expression(0)?;
                count += 1;
                if !self.eat(",") {
                    break;
                }
            }
        }
        self.expect(")")?;
        Ok(count)
    }
    fn call_name(&mut self, first: String) -> Result<String, Fault> {
        if self.eat("::") {
            let module = normalize_module(&first).map_err(|m| self.error(m))?;
            self.dependencies.insert(module.clone());
            Ok(format!("{module}::{}", self.ident()?.to_ascii_lowercase()))
        } else {
            Ok(first.to_ascii_lowercase())
        }
    }
    fn invocation(&mut self, method: bool, spawn: bool) -> Result<(), Fault> {
        let location = self.location();
        if self.eat("[") {
            self.expect("[")?;
            self.expression(0)?;
            self.expect("]")?;
            self.expect("]")?;
            let argc = self.arguments()?;
            self.code
                .push((location, Op::Indirect(argc, method, spawn)));
        } else {
            let first = self.ident()?;
            let name = self.call_name(first)?;
            if method
                && !spawn
                && ["notify", "waittill", "waittillmatch", "endon"].contains(&name.as_str())
            {
                self.expect("(")?;
                self.expression(0)?;
                match name.as_str() {
                    "waittill" => {
                        let mut outputs = Vec::new();
                        while self.eat(",") {
                            let output = self.ident()?.to_ascii_lowercase();
                            outputs.push(self.slot(&output));
                        }
                        self.emit(Op::Await(outputs));
                    }
                    "waittillmatch" => {
                        let mut argc = 0;
                        while self.eat(",") {
                            self.expression(0)?;
                            argc += 1;
                        }
                        self.emit(Op::AwaitMatch(argc));
                    }
                    "notify" => {
                        let mut argc = 0;
                        while self.eat(",") {
                            self.expression(0)?;
                            argc += 1;
                        }
                        self.emit(Op::Notify(argc));
                    }
                    _ => {
                        self.emit(Op::Endon);
                    }
                }
                self.expect(")")?;
                self.emit(Op::Constant(Value::Undefined));
            } else {
                let site = match (spawn, method) {
                    (true, _) => CallSite::Spawn,
                    (false, true) => CallSite::Call(Namespace::Method),
                    (false, false) => CallSite::Call(Namespace::Function),
                };
                let callee = Callee::Unlinked(self.unlinked(name, site));
                let argc = self.arguments()?;
                self.code.push((
                    location,
                    if spawn {
                        Op::Spawn(callee, argc, method)
                    } else {
                        Op::Call(callee, argc, method)
                    },
                ));
            }
        }
        Ok(())
    }
    fn expression(&mut self, min: u8) -> Result<(), Fault> {
        let location = self.location();
        if self.eat("call") {
            self.invocation(false, false)?;
        } else if self.eat("thread") {
            self.invocation(false, true)?;
        } else if self.eat("%") {
            let name = self.ident()?.to_ascii_lowercase();
            let tree = self
                .animtree
                .clone()
                .ok_or_else(|| self.error("animation literal without an animation tree"))?;
            self.emit(Op::Constant(Value::Animation {
                tree: tree.into(),
                name: name.into(),
            }));
        } else if self.eat("#") {
            self.expect("animtree")?;
            let tree = self
                .animtree
                .clone()
                .ok_or_else(|| self.error("animation tree is not declared"))?;
            self.emit(Op::Constant(Value::AnimationTree(tree.into())));
        } else if self.eat("::") {
            let name = self.ident()?.to_ascii_lowercase();
            let id = self.unlinked(name, CallSite::Reference);
            self.emit(Op::FunctionRef(id));
        } else if self.eat("&") {
            let t = self.tokens[self.pos].clone();
            if !t.string {
                return Err(self.error("expected localized string"));
            }
            self.pos += 1;
            self.emit(Op::Constant(Value::LocalizedString(t.text.into())));
        } else if self.eat("[") {
            if self.eat("[") {
                self.expression(0)?;
                self.expect("]")?;
                self.expect("]")?;
                let argc = self.arguments()?;
                self.emit(Op::Indirect(argc, false, false));
            } else {
                self.expect("]")?;
                self.emit(Op::Array);
            }
        } else if self.eat("~") {
            self.expression(12)?;
            self.emit(Op::Unary(Unary::Complement));
        } else if self.eat("!") {
            self.expression(12)?;
            self.emit(Op::Unary(Unary::Not));
        } else if self.eat("-") {
            let t = &self.tokens[self.pos];
            let literal = (!t.string)
                .then(|| self.number(&format!("-{}", t.text)))
                .flatten();
            if let Some(value) = literal {
                let value = value?;
                self.pos += 1;
                self.emit(Op::Constant(value));
            } else {
                self.emit(Op::Constant(Value::Int(0)));
                self.expression(12)?;
                self.emit(Op::Binary(Binary::Sub));
            }
        } else if self.eat("(") {
            self.expression(0)?;
            if self.eat(",") {
                self.expression(0)?;
                self.expect(",")?;
                self.expression(0)?;
                self.emit(Op::Vector);
            }
            self.expect(")")?;
        } else {
            let t = self.tokens[self.pos].clone();
            if t.string {
                self.pos += 1;
                self.emit(Op::Constant(Value::String(t.text.into())));
            } else if let Some(value) = self.number(&t.text) {
                let value = value?;
                self.pos += 1;
                self.emit(Op::Constant(value));
            } else if self.eat("undefined") {
                self.emit(Op::Constant(Value::Undefined));
            } else if self.eat("true") {
                self.emit(Op::Constant(Value::Int(1)));
            } else if self.eat("false") {
                self.emit(Op::Constant(Value::Int(0)));
            } else {
                let first = self.ident()?;
                let name = self.call_name(first)?;
                if self.is("(") {
                    let site = CallSite::Call(Namespace::Function);
                    let callee = Callee::Unlinked(self.unlinked(name, site));
                    let argc = self.arguments()?;
                    self.code
                        .push((location.clone(), Op::Call(callee, argc, false)));
                } else if name.contains("::") || name.contains('\\') {
                    let id = self.unlinked(name, CallSite::Reference);
                    self.emit(Op::FunctionRef(id));
                } else if let Some(value) = self.constants.get(&name).cloned() {
                    self.emit(Op::Constant(value));
                } else {
                    let op = self.load(&name);
                    self.emit(op);
                }
            }
        }
        loop {
            if self.eat(".") {
                let field = self.ident()?.to_ascii_lowercase();
                if field == "size" {
                    self.emit(Op::Size);
                } else {
                    let field = self.symbol(&field);
                    self.emit(Op::LoadField(field));
                }
            } else if self.is("[") && !self.tokens.get(self.pos + 1).is_some_and(|t| t.text == "[")
            {
                self.pos += 1;
                self.expression(0)?;
                self.expect("]")?;
                self.emit(Op::LoadIndex);
            } else if self.eat("call") {
                self.invocation(true, false)?;
            } else if self.eat("thread") {
                self.invocation(true, true)?;
            } else {
                let receiver_call = !self.tokens[self.pos].string
                    && self.tokens[self.pos]
                        .text
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && self
                        .tokens
                        .get(self.pos + 1)
                        .is_some_and(|t| t.text == "(" || t.text == "::");
                if receiver_call
                    || (self.is("[")
                        && self.tokens.get(self.pos + 1).is_some_and(|t| t.text == "["))
                {
                    self.invocation(true, false)?;
                } else {
                    break;
                }
            }
        }
        loop {
            let op = self.tokens[self.pos].text.clone();
            let (power, binary) = match op.as_str() {
                "||" => (1, None),
                "&&" => (2, None),
                "|" => (3, Some(Binary::Or)),
                "^" => (4, Some(Binary::Xor)),
                "&" => (5, Some(Binary::And)),
                "==" => (6, Some(Binary::Equal)),
                "!=" => (6, Some(Binary::NotEqual)),
                "<" => (7, Some(Binary::Less)),
                ">" => (7, Some(Binary::Greater)),
                "<=" => (7, Some(Binary::LessEqual)),
                ">=" => (7, Some(Binary::GreaterEqual)),
                "<<" => (8, Some(Binary::Shl)),
                ">>" => (8, Some(Binary::Shr)),
                "+" => (9, Some(Binary::Add)),
                "-" => (9, Some(Binary::Sub)),
                "*" => (10, Some(Binary::Mul)),
                "/" => (10, Some(Binary::Div)),
                "%" => (10, Some(Binary::Mod)),
                _ => break,
            };
            if power < min {
                break;
            }
            self.pos += 1;
            if let Some(binary) = binary {
                self.expression(power + 1)?;
                self.code.push((location.clone(), Op::Binary(binary)));
            } else {
                self.emit(Op::Unary(Unary::Not));
                self.emit(Op::Unary(Unary::Not));
                if op == "||" {
                    self.emit(Op::Unary(Unary::Not));
                }
                let branch = self.emit(Op::JumpFalse(0));
                self.expression(power + 1)?;
                self.emit(Op::Unary(Unary::Not));
                self.emit(Op::Unary(Unary::Not));
                let done = self.emit(Op::Jump(0));
                self.patch(branch, self.code.len());
                self.emit(Op::Constant(Value::Int(i32::from(op == "||"))));
                self.patch(done, self.code.len());
            }
        }
        if min == 0 && self.eat("?") {
            let alternative = self.emit(Op::JumpFalse(0));
            self.expression(0)?;
            self.expect(":")?;
            let done = self.emit(Op::Jump(0));
            self.patch(alternative, self.code.len());
            self.expression(0)?;
            self.patch(done, self.code.len());
        }
        Ok(())
    }
    fn assignment_ahead(&self) -> bool {
        let mut pos = self.pos + 1;
        loop {
            if self
                .tokens
                .get(pos)
                .is_some_and(|t| !t.string && t.text == ".")
            {
                pos += 2;
            } else if self
                .tokens
                .get(pos)
                .is_some_and(|t| !t.string && t.text == "[")
            {
                let mut depth = 1;
                pos += 1;
                while let Some(t) = self.tokens.get(pos) {
                    if !t.string && t.text == "[" {
                        depth += 1;
                    }
                    if !t.string && t.text == "]" {
                        depth -= 1;
                    }
                    pos += 1;
                    if depth == 0 {
                        break;
                    }
                    if t.text.is_empty() {
                        return false;
                    }
                }
            } else {
                break;
            }
        }
        self.tokens.get(pos).is_some_and(|t| {
            !t.string
                && [
                    "=", "+=", "-=", "*=", "/=", "|=", "&=", "^=", "%=", "++", "--",
                ]
                .contains(&t.text.as_str())
        })
    }
    fn assignment(&mut self) -> Result<(), Fault> {
        let name = self.ident()?.to_ascii_lowercase();
        if self.constants.contains_key(&name) {
            return Err(self.error("cannot assign a constant"));
        }
        let mut target = self.load(&name);
        if self.is(".") || self.is("[") {
            if let (true, Op::Load(slot)) = (self.is("["), &target) {
                self.emit(Op::EnsureLocalArray(*slot));
            }
            self.emit(target.clone());
            loop {
                let load;
                if self.eat(".") {
                    let field = self.ident()?.to_ascii_lowercase();
                    let field = self.symbol(&field);
                    target = Op::StoreField(field);
                    load = Op::LoadField(field);
                } else if self.eat("[") {
                    self.expression(0)?;
                    self.expect("]")?;
                    target = Op::StoreIndex;
                    load = Op::LoadIndex;
                } else {
                    break;
                }
                if self.is(".") || self.is("[") {
                    let load = if self.is("[") {
                        match load {
                            Op::LoadField(n) => Op::EnsureFieldArray(n),
                            Op::LoadIndex => Op::EnsureIndexArray,
                            _ => unreachable!(),
                        }
                    } else {
                        load
                    };
                    self.emit(load);
                } else {
                    break;
                }
            }
        } else if let Op::Global(_) = target {
            return Err(self.error("cannot assign global receiver"));
        } else if let Op::Load(slot) = target {
            target = Op::Store(slot);
        }
        let op = self.tokens[self.pos].text.clone();
        self.pos += 1;
        if op != "=" {
            match &target {
                Op::Store(n) => {
                    self.emit(Op::Load(*n));
                }
                Op::StoreField(n) => {
                    self.emit(Op::Dup);
                    self.emit(Op::LoadField(*n));
                }
                Op::StoreIndex => {
                    self.emit(Op::DupPair);
                    self.emit(Op::LoadIndex);
                }
                _ => unreachable!(),
            }
        }
        if op == "++" || op == "--" {
            self.emit(Op::Constant(Value::Int(1)));
        } else {
            self.expression(0)?;
        }
        if op != "=" {
            self.emit(Op::Binary(match &op[..1] {
                "+" => Binary::Add,
                "-" => Binary::Sub,
                "*" => Binary::Mul,
                "/" => Binary::Div,
                "%" => Binary::Mod,
                "|" => Binary::Or,
                "&" => Binary::And,
                _ => Binary::Xor,
            }));
        }
        self.emit(target);
        Ok(())
    }
    fn statement(&mut self) -> Result<(), Fault> {
        if self.eat("{") {
            while !self.eat("}") {
                if self.is("") {
                    return Err(self.error("unterminated block"));
                }
                self.statement()?;
            }
        } else if self.eat(";") {
        } else if self.eat("if") {
            self.expect("(")?;
            self.expression(0)?;
            self.expect(")")?;
            let branch = self.emit(Op::JumpFalse(0));
            self.statement()?;
            if self.eat("else") {
                let done = self.emit(Op::Jump(0));
                self.patch(branch, self.code.len());
                self.statement()?;
                self.patch(done, self.code.len());
            } else {
                self.patch(branch, self.code.len());
            }
        } else if self.eat("foreach") {
            self.expect("(")?;
            let first = self.ident()?.to_ascii_lowercase();
            let (key, value) = if self.eat(",") {
                (Some(first), self.ident()?.to_ascii_lowercase())
            } else {
                (None, first)
            };
            if key
                .iter()
                .chain(std::iter::once(&value))
                .any(|n| ["self", "level", "game"].contains(&n.as_str()))
            {
                return Err(self.error("cannot assign global receiver"));
            }
            self.expect("in")?;
            self.expression(0)?;
            self.expect(")")?;
            let array = self.slot(&format!("$array{}", self.pos));
            let keys = self.slot(&format!("$keys{}", self.pos));
            let cursor = self.slot(&format!("$cursor{}", self.pos));
            self.emit(Op::Dup);
            self.emit(Op::Store(array));
            self.emit(Op::ArrayKeys);
            self.emit(Op::Store(keys));
            self.emit(Op::Constant(Value::Int(0)));
            self.emit(Op::Store(cursor));
            let condition = self.code.len();
            self.emit(Op::Load(cursor));
            self.emit(Op::Load(keys));
            self.emit(Op::Size);
            self.emit(Op::Binary(Binary::Less));
            let done = self.emit(Op::JumpFalse(0));
            self.emit(Op::Load(array));
            self.emit(Op::Load(keys));
            self.emit(Op::Load(cursor));
            self.emit(Op::LoadIndex);
            if let Some(key) = key {
                let key = self.slot(&key);
                self.emit(Op::Dup);
                self.emit(Op::Store(key));
            }
            self.emit(Op::LoadIndex);
            let value = self.slot(&value);
            self.emit(Op::Store(value));
            self.loops.push((Vec::new(), Some(Vec::new())));
            self.statement()?;
            let increment = self.code.len();
            self.emit(Op::Load(cursor));
            self.emit(Op::Constant(Value::Int(1)));
            self.emit(Op::Binary(Binary::Add));
            self.emit(Op::Store(cursor));
            self.emit(Op::Jump(condition));
            let end = self.code.len();
            self.patch(done, end);
            let (breaks, continues) = self.loops.pop().unwrap();
            for pc in breaks {
                self.patch(pc, end);
            }
            for pc in continues.unwrap() {
                self.patch(pc, increment);
            }
        } else if self.eat("switch") {
            self.expect("(")?;
            self.expression(0)?;
            self.expect(")")?;
            let selector = self.slot(&format!("$switch{}", self.pos));
            self.emit(Op::Store(selector));
            self.expect("{")?;
            let dispatch = self.emit(Op::Jump(0));
            let mut cases = Vec::new();
            let mut default = None;
            self.loops.push((Vec::new(), None));
            while !self.eat("}") {
                if self.eat("case") {
                    let t = self.tokens[self.pos].clone();
                    let negative = self.eat("-");
                    let value = if t.string {
                        self.pos += 1;
                        Value::String(t.text.into())
                    } else {
                        let text = &self.tokens[self.pos].text;
                        let text = if negative {
                            format!("-{text}")
                        } else {
                            text.clone()
                        };
                        let value = match self.number(&text) {
                            Some(Ok(value @ Value::Int(_))) => value,
                            _ => return Err(self.error("case label must be an integer or string")),
                        };
                        self.pos += 1;
                        value
                    };
                    if cases.iter().any(|(v, _)| *v == value) {
                        return Err(self.error("duplicate case label"));
                    }
                    self.expect(":")?;
                    cases.push((value, self.code.len()));
                } else if self.eat("default") {
                    self.expect(":")?;
                    if default.replace(self.code.len()).is_some() {
                        return Err(self.error("duplicate default"));
                    }
                } else {
                    if self.is("") {
                        return Err(self.error("unterminated switch"));
                    }
                    self.statement()?;
                }
            }
            let finish = self.emit(Op::Jump(0));
            self.patch(dispatch, self.code.len());
            for (value, target) in cases {
                self.emit(Op::Load(selector));
                self.emit(Op::Constant(value));
                self.emit(Op::Binary(Binary::Case));
                let next = self.emit(Op::JumpFalse(0));
                self.emit(Op::Jump(target));
                self.patch(next, self.code.len());
            }
            let otherwise = self.emit(Op::Jump(default.unwrap_or(0)));
            let end = self.code.len();
            if default.is_none() {
                self.patch(otherwise, end);
            }
            self.patch(finish, end);
            let (breaks, _) = self.loops.pop().unwrap();
            for pc in breaks {
                self.patch(pc, end);
            }
        } else if self.eat("for") {
            self.expect("(")?;
            if !self.is(";") {
                self.assignment()?;
            }
            self.expect(";")?;
            let condition = self.code.len();
            if self.is(";") {
                self.emit(Op::Constant(Value::Int(1)));
            } else {
                self.expression(0)?;
            }
            self.expect(";")?;
            let done = self.emit(Op::JumpFalse(0));
            let body = self.emit(Op::Jump(0));
            let increment = self.code.len();
            if !self.is(")") {
                self.assignment()?;
            }
            self.expect(")")?;
            self.emit(Op::Jump(condition));
            self.patch(body, self.code.len());
            self.loops.push((Vec::new(), Some(Vec::new())));
            self.statement()?;
            self.emit(Op::Jump(increment));
            let end = self.code.len();
            self.patch(done, end);
            let (breaks, continues) = self.loops.pop().unwrap();
            for pc in breaks {
                self.patch(pc, end);
            }
            for pc in continues.unwrap() {
                self.patch(pc, increment);
            }
        } else if self.eat("while") {
            let condition = self.code.len();
            self.expect("(")?;
            self.expression(0)?;
            self.expect(")")?;
            let done = self.emit(Op::JumpFalse(0));
            self.loops.push((Vec::new(), Some(Vec::new())));
            self.statement()?;
            self.emit(Op::Jump(condition));
            let end = self.code.len();
            self.patch(done, end);
            let (breaks, continues) = self.loops.pop().unwrap();
            for pc in breaks {
                self.patch(pc, end);
            }
            for pc in continues.unwrap() {
                self.patch(pc, condition);
            }
        } else if self.is("break") || self.is("continue") {
            let is_break = self.eat("break");
            if !is_break {
                self.expect("continue")?;
            }
            if self.loops.is_empty() {
                return Err(self.error("break/continue outside loop"));
            }
            let pc = self.emit(Op::Jump(0));
            if is_break {
                self.loops.last_mut().unwrap().0.push(pc);
            } else {
                self.loops
                    .iter_mut()
                    .rev()
                    .find_map(|(_, c)| c.as_mut())
                    .ok_or_else(|| Fault::at(&self.code[pc].0, "continue outside loop"))?
                    .push(pc);
            }
            self.expect(";")?;
        } else if self.eat("return") {
            if self.is(";") {
                self.emit(Op::Constant(Value::Undefined));
            } else {
                self.expression(0)?;
            }
            self.emit(Op::Return);
            self.expect(";")?;
        } else if self.eat("waittillframeend") {
            self.emit(Op::FrameEnd);
            self.expect(";")?;
        } else if self.eat("wait") {
            self.expression(0)?;
            self.emit(Op::Wait);
            self.expect(";")?;
        } else if self.assignment_ahead() {
            self.assignment()?;
            self.expect(";")?;
        } else if (self.is("prof_begin") || self.is("prof_end"))
            && self.tokens.get(self.pos + 1).is_some_and(|t| t.text == "(")
        {
            self.pos += 2;
            if !self.tokens[self.pos].string {
                return Err(self.error("expected profile name string"));
            }
            self.pos += 1;
            self.expect(")")?;
            self.expect(";")?;
        } else {
            let start = self.code.len();
            self.expression(0)?;
            if let Some((_, Op::Call(Callee::Unlinked(id), ..))) = self.code.last() {
                self.tables.pending[*id as usize].statement = Some(start);
            }
            self.emit(Op::Pop);
            self.expect(";")?;
        }
        Ok(())
    }
    fn constant(&mut self, name: String) -> Result<(), Fault> {
        let start = self.code.len();
        self.expression(0)?;
        let code = self.code.split_off(start);
        let mut values = Vec::new();
        for (location, op) in code {
            let pop = |values: &mut Vec<Value>| {
                values
                    .pop()
                    .ok_or_else(|| Fault::at(&location, "invalid constant expression"))
            };
            match op {
                Op::Constant(value) => values.push(value),
                Op::Unary(op) => {
                    let value = pop(&mut values)?;
                    values.push(
                        super::runtime::unary(op, value).map_err(|e| Fault::at(&location, e))?,
                    );
                }
                Op::Binary(op) => {
                    let b = pop(&mut values)?;
                    let a = pop(&mut values)?;
                    values.push(
                        super::runtime::binary(op, a, b).map_err(|e| Fault::at(&location, e))?,
                    );
                }
                _ => {
                    return Err(Fault::at(
                        &location,
                        "constant requires a literal expression",
                    ));
                }
            }
        }
        if values.len() != 1 {
            return Err(self.error("invalid constant expression"));
        }
        if self.constants.insert(name, values.pop().unwrap()).is_some() {
            return Err(self.error("duplicate constant"));
        }
        Ok(())
    }
    fn module(&mut self) -> Result<Vec<Function>, Fault> {
        let mut functions = Vec::new();
        while !self.is("") {
            if self.eat("#") {
                if self.eat("define") {
                    let name = self.ident()?.to_ascii_lowercase();
                    let line = self.tokens[self.pos - 1].line;
                    if let Some(end) = (self.pos..self.tokens.len())
                        .find(|&i| self.tokens[i].line > line || self.tokens[i].text.is_empty())
                    {
                        let mut separator = self.tokens[end].clone();
                        separator.text = ";".into();
                        separator.string = false;
                        self.tokens.insert(end, separator);
                    }
                    self.constant(name)?;
                    self.eat(";");
                    continue;
                }
                if self.eat("using_animtree") {
                    self.expect("(")?;
                    let token = self.tokens[self.pos].clone();
                    if !token.string {
                        return Err(self.error("expected animation tree string"));
                    }
                    self.pos += 1;
                    self.animtree = Some(token.text);
                    self.expect(")")?;
                    self.expect(";")?;
                    continue;
                }
                self.expect("include")?;
                let name = self.ident()?;
                let module = normalize_module(&name).map_err(|m| self.error(m))?;
                self.includes.push(module.clone());
                self.dependencies.insert(module);
                self.expect(";")?;
                continue;
            }
            self.function = self.ident()?.to_ascii_lowercase();
            if self.eat("=") {
                self.constant(self.function.clone())?;
                self.expect(";")?;
                continue;
            }
            let location = self.location();
            self.expect("(")?;
            self.slots.clear();
            let mut parameters = 0;
            if !self.is(")") {
                loop {
                    let parameter = self.ident()?.to_ascii_lowercase();
                    if self.slots.contains_key(&parameter)
                        || ["self", "level", "game"].contains(&parameter.as_str())
                    {
                        return Err(self.error("duplicate parameter"));
                    }
                    self.slot(&parameter);
                    parameters += 1;
                    if !self.eat(",") {
                        break;
                    }
                }
            }
            self.expect(")")?;
            if !self.is("{") {
                return Err(self.error("expected function body"));
            }
            self.statement()?;
            self.emit(Op::Constant(Value::Undefined));
            self.emit(Op::Return);
            functions.push(Function {
                location,
                parameters,
                slots: self.slots.len(),
                code: std::mem::take(&mut self.code),
            });
        }
        Ok(functions)
    }
}

pub(super) fn compile(
    resolver: &impl SourceResolver,
    roots: &[&str],
    catalog: &Catalog,
) -> Result<Program, Fault> {
    let root_location = Location {
        module: "<loader>".into(),
        function: String::new(),
        line: 1,
        column: 1,
    };
    if roots.is_empty() {
        return Err(Fault::at(&root_location, "no script roots"));
    }
    let mut pending = BTreeSet::new();
    for root in roots {
        pending.insert(normalize_module(root).map_err(|m| Fault::at(&root_location, m))?);
    }
    let mut functions = Vec::new();
    let mut names = BTreeMap::new();
    let mut modules = Vec::new();
    let mut tables = Tables::default();
    let mut imports = BTreeMap::new();
    while let Some(module) = pending.pop_first() {
        if imports.contains_key(&module) {
            continue;
        }
        let location = Location {
            module: module.clone(),
            ..root_location.clone()
        };
        let bytes = resolver
            .read_bytes(&module)
            .map_err(|m| Fault::at(&location, m))?;
        let source = decode_source(&bytes);
        let mut parser = Parser {
            tables: std::mem::take(&mut tables),
            slots: BTreeMap::new(),
            module: module.clone(),
            function: String::new(),
            tokens: lex(&module, &source)?,
            pos: 0,
            code: Vec::new(),
            includes: Vec::new(),
            dependencies: BTreeSet::new(),
            loops: Vec::new(),
            constants: BTreeMap::new(),
            animtree: None,
        };
        for function in parser.module()? {
            let key = format!("{}::{}", module, function.location.function);
            if names.insert(key, functions.len()).is_some() {
                return Err(Fault::at(&function.location, "duplicate function"));
            }
            functions.push(function);
        }
        tables = parser.tables;
        imports.insert(module.clone(), parser.includes);
        pending.extend(parser.dependencies);
        modules.push(ModuleIdentity {
            site: Site::Server,
            realm: Realm::Iw4,
            module,
            sha256: Sha256::digest(&bytes).into(),
        });
    }
    let mut native_ids = BTreeMap::new();
    let mut natives = Vec::new();
    let mut resolved = Vec::with_capacity(tables.pending.len());
    for pending in &tables.pending {
        resolved.push(link(pending, &names, &imports, catalog).map(|link| {
            match link {
                Link::Builtin(builtin) => Link::Callee(Callee::Native(
                    *native_ids
                        .entry((builtin.namespace, builtin.name))
                        .or_insert_with(|| {
                            natives.push(builtin.clone());
                            natives.len() as u32 - 1
                        }),
                )),
                other => other,
            }
        }));
    }
    for function in &mut functions {
        let mut elided = Vec::new();
        for (pc, (_, op)) in function.code.iter().enumerate() {
            if let Op::Call(Callee::Unlinked(id), ..) = op
                && let Ok(Link::Elide) = resolved[*id as usize]
            {
                elided.push(tables.pending[*id as usize].statement.unwrap()..pc + 2);
            }
        }
        if !elided.is_empty() {
            let mut keep = vec![true; function.code.len()];
            for range in elided {
                keep[range].fill(false);
            }
            let mut remap = Vec::with_capacity(keep.len() + 1);
            let mut next = 0;
            for &kept in &keep {
                remap.push(next);
                next += usize::from(kept);
            }
            remap.push(next);
            let mut kept = keep.iter();
            function.code.retain(|_| *kept.next().unwrap());
            for (_, op) in &mut function.code {
                if let Op::Jump(to) | Op::JumpFalse(to) = op {
                    *to = remap[*to];
                }
            }
        }
        for (location, op) in &mut function.code {
            let (id, argc) = match op {
                Op::Call(Callee::Unlinked(id), argc, _)
                | Op::Spawn(Callee::Unlinked(id), argc, _) => (*id, *argc),
                Op::FunctionRef(id) => (*id, 0),
                _ => continue,
            };
            let callee = match resolved[id as usize]
                .clone()
                .map_err(|m| Fault::at(location, m))?
            {
                Link::Callee(callee) => callee,
                Link::Builtin(_) | Link::Elide => unreachable!(),
            };
            if matches!(callee, Callee::Native(_)) && argc >= 256 {
                return Err(Fault::at(location, "parameter count exceeds 256"));
            }
            match op {
                Op::Call(slot, ..) | Op::Spawn(slot, ..) => *slot = callee,
                _ => {
                    *op = Op::Constant(match callee {
                        Callee::Script(target) => Value::Function(target),
                        Callee::Native(target) => Value::Builtin(target),
                        Callee::Unlinked(_) => unreachable!(),
                    })
                }
            }
        }
    }
    modules.sort_by(|a, b| a.module.cmp(&b.module));
    Ok(Program {
        functions,
        names,
        modules,
        symbols: tables.symbols,
        symbol_ids: tables.symbol_ids,
        natives,
    })
}

#[derive(Clone)]
enum Link {
    Callee(Callee),
    Builtin(Builtin),
    Elide,
}

fn link(
    pending: &Pending,
    names: &BTreeMap<String, usize>,
    imports: &BTreeMap<String, Vec<String>>,
    catalog: &Catalog,
) -> Result<Link, String> {
    let Pending {
        module,
        name,
        site,
        statement,
    } = pending;
    let script = |key: &str| {
        names
            .get(key)
            .map(|&id| Link::Callee(Callee::Script(id as u32)))
    };
    if name.contains("::") {
        return script(name).ok_or_else(|| format!("unresolved function {name}"));
    }
    if let Some(local) = script(&format!("{module}::{name}")) {
        return Ok(local);
    }
    let builtin = match site {
        CallSite::Call(namespace) => catalog.get(*namespace, name),
        CallSite::Reference => catalog
            .get(Namespace::Function, name)
            .or_else(|| catalog.get(Namespace::Method, name)),
        CallSite::Spawn => None,
    };
    if let Some(builtin) = builtin {
        return match (builtin.developer, site, statement) {
            (false, ..) => Ok(Link::Builtin(builtin.clone())),
            (true, CallSite::Call(_), Some(_)) => Ok(Link::Elide),
            (true, CallSite::Call(_), None) => Err(format!(
                "return value of developer builtin {name} is only accessible in a developer block"
            )),
            (true, ..) => Err(format!("reference to developer builtin {name}")),
        };
    }
    let mut candidates = imports[module]
        .iter()
        .filter_map(|m| names.get(&format!("{m}::{name}")))
        .collect::<BTreeSet<_>>()
        .into_iter();
    match (candidates.next(), candidates.next()) {
        (Some(&id), None) => Ok(Link::Callee(Callee::Script(id as u32))),
        (Some(_), Some(_)) => Err(format!("ambiguous imported function {name}")),
        (None, _) => Err(match site {
            CallSite::Spawn => format!("unresolved thread function {name}"),
            CallSite::Call(Namespace::Method) => {
                format!("unresolved method or unknown builtin {name}")
            }
            _ => format!("unresolved function or unknown builtin {name}"),
        }),
    }
}
