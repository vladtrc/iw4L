use super::*;
use crate::anim::xmodel_pose::{
    BlendedSkel, PosedSmodelSurface, SkinSurfaceLayout, build_skin_layout, packed_skin_shape,
    smodel_surfaces_from_blended,
};
use render_frame::skinning::{PackedSkinShape, SkinVertexRange};
use std::sync::Arc;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct ModelKey {
    name: String,
    lod: Option<u8>,
    surfaces: Vec<u8>,
}

pub(super) struct Geometry {
    pub id: u64,
    pub surfaces: Arc<[PosedSmodelSurface]>,
    pub authored: Arc<[Option<assets::MaterialIndex>]>,
    pub parts: Vec<Surface>,
}

pub(super) struct Surface {
    pub model: usize,
    pub shape: Arc<PackedSkinShape>,
    pub ranges: Arc<[SkinVertexRange]>,
}

pub(super) struct Pose {
    pub geometry: Arc<Geometry>,
    pub palettes: Vec<Arc<[[[f32; 4]; 4]]>>,
    pub gaze_pose: Option<xmodel_runtime::ActorGazePose>,
}

#[derive(Resource, Default)]
pub(super) struct Cache {
    geometries: HashMap<Vec<ModelKey>, Arc<Geometry>>,
}

impl Cache {
    pub fn clear(&mut self) {
        self.geometries.clear();
    }
    pub fn prune(&mut self) {
        self.geometries
            .retain(|_, value| Arc::strong_count(value) > 1);
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "The pose inputs are the existing CPU pose contract."
)]
pub(super) fn pose(
    cache: &mut Cache,
    catalog: &asset_world::MapXModelSceneCatalog,
    skels: &[&asset_model::ModelSkel],
    keys: &[&str],
    dobj: &xmodel_runtime::DObj,
    request: &xmodel_runtime::DObjPoseRequest,
    gaze: Option<(&xmodel_runtime::ActorGazePose, &[Mat4])>,
    lod_view: Option<DObjLodView<'_>>,
    entries: &[dpvs_iw4::SceneEntSkinEntry],
) -> Option<Pose> {
    let skin = match gaze {
        Some((_, bones)) => dobj.skin_matrices(bones),
        None => super::script_skin_matrices(dobj, request, keys)?,
    };
    let signature: Vec<_> = skels
        .iter()
        .enumerate()
        .map(|(model, skel)| {
            let lod = submodel_camera_lod(skel, lod_view);
            let mut flags = vec![0; skel.surface_vertex_ranges.len()];
            if let Some(lod) = lod {
                for surface in skel.surfaces_for_lod(lod) {
                    let visible = dobj.surface_visible(
                        model,
                        &skel.surface_part_bits[surface],
                        &request.hide_part_bits,
                    ) && dpvs_iw4::SceneEntSkinEntry::stream_draws(
                        entries,
                        model as u16,
                        lod_local_surface(skel, lod, surface) as u16,
                    );
                    flags[surface] = u8::from(visible)
                        | (u8::from(stream_lod_surface_rigid(
                            skel,
                            lod,
                            entries,
                            model as u16,
                            surface,
                        )) << 1);
                }
            }
            ModelKey {
                name: keys.get(model).copied().unwrap_or(&skel.name).into(),
                lod,
                surfaces: flags,
            }
        })
        .collect();
    let geometry = if let Some(found) = cache.geometries.get(&signature) {
        Arc::clone(found)
    } else {
        let geometry = Arc::new(build_geometry(catalog, skels, &signature)?);
        cache.geometries.insert(signature, Arc::clone(&geometry));
        geometry
    };
    let palettes: Vec<_> = dobj
        .models
        .iter()
        .map(|model| {
            let end = model.base.checked_add(model.bone_count)?;
            Some(
                skin.get(model.base..end)?
                    .iter()
                    .map(Mat4::to_cols_array_2d)
                    .collect::<Arc<[_]>>(),
            )
        })
        .collect::<Option<_>>()?;
    for (part, surface) in geometry.parts.iter().zip(geometry.surfaces.iter()) {
        let drawn = !surface.indices.is_empty();
        if drawn && part.shape.required_bones() > dobj.models.get(part.model)?.bone_count {
            return None;
        }
    }
    // Storage bindings need a nonempty palette even for an entirely unskinned model.
    let palettes = palettes
        .into_iter()
        .map(|palette| {
            if palette.is_empty() {
                Arc::from([Mat4::IDENTITY.to_cols_array_2d()])
            } else {
                palette
            }
        })
        .collect();
    Some(Pose {
        geometry,
        palettes,
        gaze_pose: gaze.map(|(pose, _)| *pose),
    })
}

fn build_geometry(
    catalog: &asset_world::MapXModelSceneCatalog,
    skels: &[&asset_model::ModelSkel],
    keys: &[ModelKey],
) -> Option<Geometry> {
    let mut surfaces = Vec::new();
    let mut authored = Vec::new();
    let mut parts = Vec::new();
    for (model, (skel, key)) in skels.iter().zip(keys).enumerate() {
        let Some(lod) = key.lod else {
            continue;
        };
        if skel.surfaces_for_lod(lod).is_empty() {
            continue;
        }
        if skel.packed_vertices.len() != skel.positions.len() {
            return None;
        }
        let visible = |surface: usize| key.surfaces.get(surface).is_none_or(|flags| flags & 1 != 0);
        let first_surface = skel.surfaces_for_lod(lod).start;
        let posed = smodel_surfaces_from_blended(
            skel,
            BlendedSkel {
                decoded: None,
                packed: Some(skel.packed_vertices.to_vec()),
            },
            lod,
            |local| visible(first_surface + local),
        )?;
        let mut layout = build_skin_layout(skel, lod, |_| true)?;
        // The legacy whole-mesh form has one implicit surface.
        if layout.surfaces.is_empty() && !skel.positions.is_empty() {
            layout.surfaces.push(SkinSurfaceLayout {
                surface_index: 0,
                src_first_vertex: 0,
                dest_first_vertex: 0,
                vertex_count: skel.positions.len(),
                index_start: 0,
                index_count: skel.indices.len() as u32,
                visible: true,
            });
        }
        let shape = packed_skin_shape(skel, lod, &layout, |_| false);
        for surface in &posed {
            let range = layout
                .surfaces
                .iter()
                .find(|row| row.surface_index == surface.surface_index)?;
            let vertices = shape
                .vertices()
                .get(range.dest_first_vertex..range.dest_first_vertex + range.vertex_count)?;
            let shape = PackedSkinShape::new(vertices.to_vec());
            let rigid = key
                .surfaces
                .get(surface.surface_index)
                .is_some_and(|flags| flags & 2 != 0);
            parts.push(Surface {
                model,
                shape,
                ranges: vec![SkinVertexRange {
                    start: 0,
                    count: range.vertex_count as u32,
                    rigid,
                }]
                .into(),
            });
            authored.push(catalog.surface_material(
                &asset_world::MapXModelAssetKey(key.name.clone()),
                surface.surface_index,
            ));
        }
        surfaces.extend(posed);
    }
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT
        .fetch_update(
            std::sync::atomic::Ordering::Relaxed,
            std::sync::atomic::Ordering::Relaxed,
            |n| n.checked_add(1),
        )
        .expect("script geometry identity exhausted");
    Some(Geometry {
        id,
        surfaces: surfaces.into(),
        authored: authored.into(),
        parts,
    })
}
