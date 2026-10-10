pub(crate) struct T6Startup {
    pub(crate) roots: Vec<String>,
    pub(crate) entries: Vec<String>,
}

impl T6Startup {
    pub(crate) fn new(map: &str) -> Self {
        let module = format!("maps/mp/{map}");
        Self {
            entries: vec![format!("{module}::main")],
            roots: vec![module],
        }
    }
}
