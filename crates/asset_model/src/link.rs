use asset_core::{AssetEdge, AssetEdgeReason, MaterialKey, MaterialSpace, WalkLocalMaterialIndex};
use asset_material::MaterialDefinitions;

pub fn capture_xmodel_material_slots(
    authored: &[Option<WalkLocalMaterialIndex>],
    materials: Option<&MaterialDefinitions>,
) -> (Vec<Option<MaterialKey>>, Vec<AssetEdge<MaterialSpace>>) {
    let keys = authored
        .iter()
        .map(|index| {
            index.and_then(|index| {
                materials?
                    .materials
                    .get(index.get())
                    .map(|material| MaterialKey {
                        namespace: material.namespace,
                        name: material.name.to_string(),
                    })
            })
        })
        .collect::<Vec<_>>();
    let edges = capture_xmodel_material_edges(&keys, authored);
    (keys, edges)
}

pub fn capture_xmodel_material_edges(
    keys: &[Option<MaterialKey>],
    authored: &[Option<WalkLocalMaterialIndex>],
) -> Vec<AssetEdge<MaterialSpace>> {
    let n = keys.len().max(authored.len());
    (0..n)
        .map(|i| {
            let named = keys
                .get(i)
                .and_then(Option::as_ref)
                .is_some_and(|key| !key.name.is_empty());
            if authored.get(i).copied().flatten().is_some() || named {
                AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss)
            } else {
                AssetEdge::Absent
            }
        })
        .collect()
}

pub fn stamp_xmodel_material_edges(
    keys: &mut [Option<MaterialKey>],
    edges: &mut Vec<AssetEdge<MaterialSpace>>,
    authored: &[Option<WalkLocalMaterialIndex>],
    materials: &MaterialDefinitions,
) {
    let n = keys.len().max(authored.len());
    edges.resize(n, AssetEdge::Absent);
    for i in 0..n {
        let hint = keys.get(i).and_then(Option::as_ref);
        let authored_slot = authored.get(i).copied().flatten().is_some()
            || hint.is_some_and(|key| !key.name.is_empty());
        if !authored_slot {
            edges[i] = AssetEdge::Absent;
            continue;
        }
        let index = hint.and_then(|key| materials.material_index_by_key(key));
        if let Some(index) = index.filter(|index| {
            materials
                .materials
                .get(index.order())
                .is_some_and(|material| material.name.is_real() && !material.name.is_empty())
        }) {
            if keys
                .get(i)
                .is_none_or(|key| key.as_ref().is_none_or(|key| key.name.is_empty()))
                && let Some(slot) = keys.get_mut(i)
            {
                let material = &materials.materials[index.order()];
                *slot = Some(MaterialKey {
                    namespace: material.namespace,
                    name: material.name.to_string(),
                });
            }
            edges[i] = AssetEdge::bind(index, materials.zone_of(index.order()));
        } else {
            edges[i] = AssetEdge::Unresolved(AssetEdgeReason::CatalogMiss);
        }
    }
}
