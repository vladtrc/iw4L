use crate::HostClassSlot;

#[derive(Debug, Clone, Copy)]
pub struct ClassPreset {
    pub name: &'static str,
    pub primary: &'static str,
    pub primary_attachments: &'static [&'static str],
    pub secondary: &'static str,
    pub secondary_attachments: &'static [&'static str],
    pub lethal: &'static str,
    pub tactical: &'static str,
    pub perks: [&'static str; 3],
    pub deathstreak: &'static str,
}

impl From<&ClassPreset> for HostClassSlot {
    fn from(preset: &ClassPreset) -> Self {
        let owned = |list: &[&str]| list.iter().map(|value| (*value).to_owned()).collect();
        Self {
            name: preset.name.to_owned(),
            primary: preset.primary.to_owned(),
            primary_attachments: owned(preset.primary_attachments),
            secondary: preset.secondary.to_owned(),
            secondary_attachments: owned(preset.secondary_attachments),
            lethal: preset.lethal.to_owned(),
            tactical: preset.tactical.to_owned(),
            perks: preset.perks.map(str::to_owned),
            deathstreak: preset.deathstreak.to_owned(),
            camos: Default::default(),
        }
    }
}

pub fn showcase_classes() -> &'static [ClassPreset] {
    SHOWCASE.get().copied().unwrap_or(&[])
}

pub fn pick_showcase(seed: u64, count: usize) -> Vec<&'static ClassPreset> {
    let mut order: Vec<&'static ClassPreset> = showcase_classes().iter().collect();
    let mut state = seed;
    for i in (1..order.len()).rev() {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        order.swap(i, (z % (i as u64 + 1)) as usize);
    }
    order.truncate(count);
    order
}

static SHOWCASE: std::sync::OnceLock<&'static [ClassPreset]> = std::sync::OnceLock::new();

/// The builder owns cross-family presets. Refuse missing or malformed owners before publication.
pub fn install_showcase_classes(presets: &'static [ClassPreset]) -> Result<(), String> {
    for preset in presets {
        for key in [
            preset.primary,
            preset.secondary,
            preset.lethal,
            preset.tactical,
        ] {
            let Some((family, path)) = key.split_once(':') else {
                return Err(format!("preset {} has unqualified key {key}", preset.name));
            };
            if asset_core::FamilyId::parse(family).is_none()
                || !path.starts_with("weapon/")
                || path.len() == 7
            {
                return Err(format!(
                    "preset {} has unavailable weapon {key}",
                    preset.name
                ));
            }
        }
    }
    SHOWCASE
        .set(presets)
        .map_err(|_| "showcase classes already installed".to_owned())
}
