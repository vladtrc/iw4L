use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct AnimStateTable {
    pub states: BTreeMap<String, Vec<AnimStateClip>>,
}

#[derive(Clone, Debug)]
pub struct AnimStateClip {
    pub alias: Option<String>,
    pub clip: String,
}

impl AnimStateTable {
    pub fn clips<'a>(
        &'a self,
        state: &str,
        alias: Option<&'a str>,
    ) -> impl Iterator<Item = &'a str> {
        self.states
            .get(state)
            .into_iter()
            .flatten()
            .filter(move |entry| entry.alias.as_deref() == alias)
            .map(|entry| entry.clip.as_str())
    }
}
