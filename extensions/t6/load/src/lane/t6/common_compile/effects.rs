use asset_material::MaterialCatalog;

pub(in crate::lane::t6) fn bind_t6_fx(
    content: &mut super::super::T6Content,
    materials: &mut MaterialCatalog,
    catalog: &mut asset_game::FxCatalog,
    refusals: &mut Vec<String>,
) -> String {
    let bound = ext_t6_material::capture::bind_native_materials(
        &content.path,
        &content.fx_materials,
        &content.techsets,
        materials,
        refusals,
    );
    let count = content.fx.len();
    for fx in &content.fx {
        crate::presentation::fx_catalog::capture(catalog, fx);
    }
    format!(
        "t6 native effects: {count} effects, {} materials",
        bound.len()
    )
}
