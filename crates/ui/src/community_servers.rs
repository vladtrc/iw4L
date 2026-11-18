use bevy::prelude::*;

#[derive(Resource)]
pub struct CommunityServers {
    pub choices: Vec<(String, String)>,
    pub selected: String,
}

impl Default for CommunityServers {
    fn default() -> Self {
        let entries = community::communities();
        let selected = entries
            .iter()
            .position(|entry| Some(entry.path.as_path()) == community::selected_path());
        let mut choices: Vec<_> = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| {
                let name = if entries
                    .iter()
                    .filter(|other| other.name == entry.name)
                    .count()
                    > 1
                {
                    format!(
                        "{} ({})",
                        entry.name,
                        entry.path.file_name().map_or_else(
                            || entry.path.display().to_string(),
                            |name| name.to_string_lossy().into_owned()
                        )
                    )
                } else {
                    entry.name.clone()
                };
                (name, index.to_string())
            })
            .collect();
        if selected.is_none() {
            choices.insert(
                0,
                (
                    if entries.is_empty() {
                        "No community servers found"
                    } else {
                        "Choose a server"
                    }
                    .into(),
                    String::new(),
                ),
            );
        }
        Self {
            choices,
            selected: selected.map_or_else(String::new, |index| index.to_string()),
        }
    }
}

impl CommunityServers {
    pub fn display_choices(&self, counts: &[Option<u32>]) -> Vec<(String, String)> {
        self.choices
            .iter()
            .map(|(name, value)| {
                if value.is_empty() {
                    return (name.clone(), value.clone());
                }
                let count = value
                    .parse::<usize>()
                    .ok()
                    .and_then(|index| counts.get(index))
                    .copied()
                    .flatten();
                let suffix = count.map_or_else(
                    || " (-- players)".to_owned(),
                    |count| format!(" ({count} players)"),
                );
                let name = name.trim();
                let mut label: String = name.chars().take(28).collect();
                if name.chars().count() > 28 {
                    label.truncate(
                        label
                            .char_indices()
                            .nth(25)
                            .map_or(label.len(), |(index, _)| index),
                    );
                    label.push_str("...");
                }
                label.push_str(&suffix);
                (label, value.clone())
            })
            .collect()
    }

    pub fn restart(&self, choice: &str) -> Result<(), String> {
        let entry = choice
            .parse::<usize>()
            .ok()
            .and_then(|index| community::communities().get(index))
            .ok_or("Choose a community server first.")?;
        community::request_restart(&entry.path)
    }
}
