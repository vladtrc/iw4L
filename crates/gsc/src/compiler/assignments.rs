use super::Parser;
use crate::{Binary, Fault, Op, Value};

impl Parser {
    pub fn assignment_ahead(&self) -> bool {
        let mut pos = self.pos + 1;
        loop {
            if self.tokens.get(pos).is_some_and(|t| t.is(".")) {
                pos += 2;
            } else if self.tokens.get(pos).is_some_and(|t| t.is("[")) {
                let mut depth = 1;
                pos += 1;
                while let Some(t) = self.tokens.get(pos) {
                    if t.is("[") {
                        depth += 1;
                    }
                    if t.is("]") {
                        depth -= 1;
                    }
                    pos += 1;
                    if depth == 0 {
                        break;
                    }
                    if t.is("") {
                        return false;
                    }
                }
            } else {
                break;
            }
        }
        self.tokens
            .get(pos)
            .is_some_and(|t| t.assignment_operator().is_some())
    }
    pub fn assignment(&mut self) -> Result<(), Fault> {
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
        let op = self.tokens[self.pos]
            .assignment_operator()
            .ok_or_else(|| self.error("expected assignment operator"))?
            .to_owned();
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
}
