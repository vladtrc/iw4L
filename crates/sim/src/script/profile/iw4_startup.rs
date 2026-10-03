use crate::script::source::SourceResolver;

/// Install runs the struct initializer itself, before the map's structs exist.
pub struct Iw4Startup {
    pub roots: Vec<String>,
    pub entries: Vec<String>,
}
impl Iw4Startup {
    pub fn new(resolver: &impl SourceResolver, gametype: &str, map: &str) -> Self {
        let gametype = format!("maps/mp/gametypes/{gametype}");
        // `map` is a zone name: `:` is not legal in a module name, so a
        // namespaced key (`iw4l:field`) loads its map script under the stem.
        let map = format!("maps/mp/{}", map.rsplit(':').next().unwrap_or(map));
        let callbacks = "maps/mp/gametypes/_callbacksetup";
        let mut roots = vec![
            "codescripts/delete".to_owned(),
            "codescripts/struct".to_owned(),
            callbacks.to_owned(),
            gametype.clone(),
        ];
        let mut entries = vec![format!("{gametype}::main")];
        if resolver.read_bytes(&map).is_ok() {
            entries.push(format!("{map}::main"));
            roots.push(map);
        }
        entries.push(format!("{callbacks}::codecallback_startgametype"));
        Self { roots, entries }
    }
}
