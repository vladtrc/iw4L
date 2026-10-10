#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub module: String,
    pub function: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fault {
    pub location: Location,
    pub message: String,
    pub callers: Vec<Location>,
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{} in {}: {}",
            self.location.module,
            self.location.line,
            self.location.column,
            self.location.function,
            self.message
        )?;
        for caller in self.callers.iter().rev() {
            write!(
                f,
                "\n  called from {}:{}:{} in {}",
                caller.module, caller.line, caller.column, caller.function
            )?;
        }
        Ok(())
    }
}
impl std::error::Error for Fault {}

impl Fault {
    pub fn at(location: &Location, message: impl Into<String>) -> Self {
        Self {
            location: location.clone(),
            message: message.into(),
            callers: Vec::new(),
        }
    }
}
