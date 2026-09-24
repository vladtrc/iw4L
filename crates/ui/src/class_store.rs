use bevy::prelude::*;

use crate::class_presets::default_presets;
use crate::class_setup::{ClassSetupScratch, ClassSlotState};
use frame::{HostClassLoadouts, HostClassSlot};

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct SessionClassStore {
    pub slots: Vec<ClassSlotState>,

    pub equipped: Option<usize>,
    pub selected: usize,
}

impl Default for SessionClassStore {
    fn default() -> Self {
        Self::from_presets()
    }
}

impl SessionClassStore {
    pub fn from_presets() -> Self {
        Self {
            slots: default_presets()
                .iter()
                .map(ClassSlotState::from_preset)
                .collect(),
            equipped: None,
            selected: 0,
        }
    }

    pub fn commit_slots(&mut self, scratch: &ClassSetupScratch) {
        self.slots = scratch.slots.clone();
        self.selected = scratch.selected;
    }

    pub fn commit_equip(&mut self, selected: usize) {
        if selected < self.slots.len() {
            self.selected = selected;
            self.equipped = Some(selected);
        }
    }

    pub fn equipped_slot(&self) -> Option<&ClassSlotState> {
        self.equipped.and_then(|i| self.slots.get(i))
    }

    pub fn apply_coverage_locks(&mut self, reasons: impl IntoIterator<Item = Option<String>>) {
        for (slot, reason) in self.slots.iter_mut().zip(reasons) {
            slot.lock_reason = reason;
        }
    }
}

impl From<&ClassSlotState> for HostClassSlot {
    fn from(slot: &ClassSlotState) -> Self {
        Self {
            name: slot.name.clone(),
            primary: slot.primary.clone(),
            primary_attachments: slot.primary_attachments.clone(),
            secondary: slot.secondary.clone(),
            secondary_attachments: slot.secondary_attachments.clone(),
            lethal: slot.lethal.clone(),
            tactical: slot.tactical.clone(),
            perks: [slot.perk1.clone(), slot.perk2.clone(), slot.perk3.clone()],
            deathstreak: slot.deathstreak.clone(),
        }
    }
}

pub(crate) fn sync_host_class_loadouts(
    scratch: Res<ClassSetupScratch>,
    mut store: ResMut<SessionClassStore>,
    mut host: ResMut<HostClassLoadouts>,
) {
    if !scratch.is_changed() {
        return;
    }
    store.commit_slots(&scratch);
    host.slots = scratch.slots.iter().map(HostClassSlot::from).collect();
}

const CLASS_FILE_HEADER: &str = "iw4l-classes 1";

#[derive(Resource, Default)]
pub(crate) struct ClassStoreFile {
    path: Option<std::path::PathBuf>,
    loaded: bool,
    written: Option<String>,
    retry_at: Option<std::time::Instant>,
}

fn clean_field(value: &str) -> String {
    value.replace(['\t', '\n', '\r'], " ")
}

fn encode_slots(slots: &[ClassSlotState]) -> String {
    let mut out = String::from(CLASS_FILE_HEADER);
    out.push('\n');
    for slot in slots {
        let attachments = |list: &[String]| {
            list.iter()
                .map(|value| clean_field(value).replace(',', " "))
                .collect::<Vec<_>>()
                .join(",")
        };
        let fields = [
            clean_field(&slot.name),
            clean_field(&slot.primary),
            attachments(&slot.primary_attachments),
            clean_field(&slot.secondary),
            attachments(&slot.secondary_attachments),
            clean_field(&slot.lethal),
            clean_field(&slot.tactical),
            clean_field(&slot.perk1),
            clean_field(&slot.perk2),
            clean_field(&slot.perk3),
            clean_field(&slot.deathstreak),
        ];
        out.push_str(&fields.join("\t"));
        out.push('\n');
    }
    out
}

fn decode_slots(text: &str) -> Option<Vec<ClassSlotState>> {
    let mut lines = text.lines();
    if lines.next()? != CLASS_FILE_HEADER {
        return None;
    }
    let attachments = |field: &str| -> Vec<String> {
        field
            .split(',')
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect()
    };
    let mut slots = Vec::new();
    for line in lines.filter(|line| !line.is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        let [
            name,
            primary,
            primary_attachments,
            secondary,
            secondary_attachments,
            lethal,
            tactical,
            perk1,
            perk2,
            perk3,
            deathstreak,
        ] = fields.as_slice()
        else {
            return None;
        };
        slots.push(ClassSlotState {
            name: (*name).to_owned(),
            primary: (*primary).to_owned(),
            primary_attachments: attachments(*primary_attachments),
            secondary: (*secondary).to_owned(),
            secondary_attachments: attachments(*secondary_attachments),
            lethal: (*lethal).to_owned(),
            tactical: (*tactical).to_owned(),
            perk1: (*perk1).to_owned(),
            perk2: (*perk2).to_owned(),
            perk3: (*perk3).to_owned(),
            deathstreak: (*deathstreak).to_owned(),
            lock_reason: None,
        });
    }
    (!slots.is_empty()).then_some(slots)
}

pub(crate) fn load_class_store(
    identity: Option<Res<frame::LaunchIdentity>>,
    mut file: ResMut<ClassStoreFile>,
    mut scratch: ResMut<ClassSetupScratch>,
) {
    if file.loaded {
        return;
    }
    let Some(identity) = identity else {
        return;
    };
    if identity.artifacts.as_os_str().is_empty() {
        return;
    }
    let path = identity.artifacts.join("profile").join("classes.txt");
    file.loaded = true;
    match std::fs::read_to_string(&path) {
        Ok(text) => match decode_slots(&text) {
            Some(slots) => {
                diag::info!(Ui, "classes: {} read from {}", slots.len(), path.display());
                file.written = Some(encode_slots(&slots));
                scratch.slots = slots;
                scratch.reset_navigation();
            }
            None => {
                diag::warn!(
                    Ui,
                    "classes: {} is not a class file this build reads; file preserved",
                    path.display()
                );
                return;
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            diag::warn!(
                Ui,
                "classes: cannot read {}: {error}; file preserved",
                path.display()
            );
            return;
        }
    }
    file.path = Some(path);
}

fn write_class_file(path: &std::path::Path, contents: &str) -> std::io::Result<()> {
    use std::io::Write;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pending = path.with_extension(format!("{}.tmp", std::process::id()));
    let result = (|| {
        let mut output = std::fs::File::create(&pending)?;
        output.write_all(contents.as_bytes())?;
        output.sync_all()?;
        drop(output);
        std::fs::rename(&pending, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&pending);
    }
    result
}

pub(crate) fn save_class_store(store: Res<SessionClassStore>, mut file: ResMut<ClassStoreFile>) {
    if !file.loaded
        || file
            .retry_at
            .is_some_and(|at| std::time::Instant::now() < at)
    {
        return;
    }
    if !store.is_changed() && file.retry_at.is_none() && file.written.is_some() {
        return;
    }
    let Some(path) = file.path.clone() else {
        return;
    };
    let contents = encode_slots(&store.slots);
    if file.written.as_ref() == Some(&contents) {
        return;
    }
    match write_class_file(&path, &contents) {
        Ok(()) => {
            file.written = Some(contents);
            file.retry_at = None;
        }
        Err(error) => {
            diag::warn!(Ui, "classes: cannot write {}: {error}", path.display());
            file.retry_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(5));
        }
    }
}
