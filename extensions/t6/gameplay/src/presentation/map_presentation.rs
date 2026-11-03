pub const POLICY: asset_game::MapPresentationFamily = asset_game::MapPresentationFamily {
    preview: |stem| format!("menu_{stem}_map_select_final"),
    table: Some(asset_game::MapTableSchema {
        name: "mp/mapstable.csv",
        map_column: 0,
        title_column: 3,
        image_column: 4,
        map_prefix: "mp_",
    }),
};
