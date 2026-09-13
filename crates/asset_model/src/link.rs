use asset_core::{AssetEdge, AssetEdgeReason, MaterialSpace, WalkLocalMaterialIndex};
use asset_material::MaterialDefinitions;

pub fn capture_xmodel_material_slots(
    authored: &[Option<WalkLocalMaterialIndex>],
    materials: Option<&MaterialDefinitions>,
) -> (Vec<Option<String>>, Vec<AssetEdge<MaterialSpace>>) {
    let names = authored
        .iter()
        .map(|index| {
            index.and_then(|index| {
                materials?
                    .materials
                    .get(index.get())
                    .map(|material| material.name.to_string())
            })
        })
        .collect::<Vec<_>>();
    let edges = capture_xmodel_material_edges(&names, authored);
    (names, edges)
}

pub fn capture_xmodel_material_edges(
    names: &[Option<String>],
    authored: &[Option<WalkLocalMaterialIndex>],
) -> Vec<AssetEdge<MaterialSpace>> {
    let n = names.len().max(authored.len());
    (0..n)
        .map(|i| {
            let named = names
                .get(i)
                .and_then(|name| name.as_deref())
                .is_some_and(|name| !name.is_empty());
            if authored.get(i).copied().flatten().is_some() || named {
                AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss)
            } else {
                AssetEdge::Absent
            }
        })
        .collect()
}

pub fn stamp_xmodel_material_edges(
    names: &mut [Option<String>],
    edges: &mut Vec<AssetEdge<MaterialSpace>>,
    authored: &[Option<WalkLocalMaterialIndex>],
    materials: &MaterialDefinitions,
) {
    let n = names.len().max(authored.len());
    edges.resize(n, AssetEdge::Absent);
    for i in 0..n {
        let hint = names.get(i).and_then(|name| name.as_deref());
        let authored_slot = authored.get(i).copied().flatten().is_some()
            || hint.is_some_and(|name| !name.is_empty());
        if !authored_slot {
            edges[i] = AssetEdge::Absent;
            continue;
        }
        let index = hint.and_then(|name| materials.material_index_by_name(name));
        if let Some(index) = index.filter(|index| {
            materials
                .materials
                .get(index.order())
                .is_some_and(|material| material.name.is_real() && !material.name.is_empty())
        }) {
            if names
                .get(i)
                .is_none_or(|name| name.as_ref().is_none_or(|name| name.is_empty()))
                && let Some(slot) = names.get_mut(i)
            {
                *slot = Some(materials.materials[index.order()].name.to_string());
            }
            edges[i] = AssetEdge::bind(index, materials.zone_of(index.order()));
        } else {
            edges[i] = AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss);
        }
    }
}
