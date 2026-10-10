use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, LazyLock, RwLock};

use asset_core::AssetNamespace;
use fastfile_iw4::GlyphCapture;

#[derive(Clone, Debug)]
pub struct UiBitmapFont {
    pub name: String,
    pub material: String,
    pub pixel_height: u32,
    pub glyphs: BTreeMap<u16, GlyphCapture>,
}

type Fonts = HashMap<(AssetNamespace, String), Arc<UiBitmapFont>>;
static FONTS: LazyLock<RwLock<Fonts>> = LazyLock::new(|| RwLock::new(HashMap::new()));

pub fn store_ui_fonts(namespace: AssetNamespace, fonts: impl IntoIterator<Item = UiBitmapFont>) {
    let mut stored = FONTS.write().unwrap_or_else(|poison| poison.into_inner());
    stored.retain(|(ns, _), _| *ns != namespace);
    stored.extend(
        fonts
            .into_iter()
            .map(|font| ((namespace, font.name.to_ascii_lowercase()), Arc::new(font))),
    );
}

pub fn ui_font(namespace: AssetNamespace, name: &str) -> Option<Arc<UiBitmapFont>> {
    FONTS
        .read()
        .unwrap_or_else(|poison| poison.into_inner())
        .get(&(namespace, name.to_ascii_lowercase()))
        .cloned()
}
