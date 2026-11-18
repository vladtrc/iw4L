pub const POLICY: asset_game::MapPresentationFamily = asset_game::MapPresentationFamily {
    singleplayer: super::singleplayer::MODES,
    preview: |stem| format!("menu_mp_map_select_{}_big", stem.trim_start_matches("mp_")),
    table: None,
};
