use super::{CallSite, Parser};
use crate::{Callee, Fault, Namespace, Op, Value, normalize_module};

impl Parser {
    pub fn arguments(&mut self) -> Result<usize, Fault> {
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
    pub fn call_name(&mut self, first: String) -> Result<String, Fault> {
        if self.eat("::") {
            let module = normalize_module(&first).map_err(|m| self.error(m))?;
            self.dependencies.insert(module.clone());
            Ok(format!("{module}::{}", self.ident()?.to_ascii_lowercase()))
        } else {
            Ok(first.to_ascii_lowercase())
        }
    }
    pub fn invocation(&mut self, method: bool, spawn: bool) -> Result<(), Fault> {
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
}
