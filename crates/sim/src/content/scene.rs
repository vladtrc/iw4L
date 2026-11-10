use super::{Digest, hash_collision};
use crate::{SimClipBsp, SimClipCmodels, SimClipMesh, SimContent};

fn sequence<T>(h: &mut Digest, values: &[T], encode: impl Fn(&mut Digest, &T)) {
    h.u64(values.len() as u64);
    for value in values {
        encode(h, value);
    }
}

fn floats<const N: usize>(h: &mut Digest, values: &[f32; N]) {
    for value in values {
        h.f32(*value);
    }
}

fn clip(
    h: &mut Digest,
    brushes: &[crate::SimBrush],
    bsp: &SimClipBsp,
    mesh: &SimClipMesh,
    models: &SimClipCmodels,
) {
    hash_collision(h, brushes);
    sequence(h, &bsp.nodes, |h, node| {
        floats(h, &node.plane);
        for child in node.children {
            h.i32(child);
        }
    });
    sequence(h, &bsp.leaves, |h, leaf| {
        h.u32(leaf.first_brush);
        h.u16(leaf.num_brushes);
        h.u16(leaf.first_coll_aabb_index);
        h.u16(leaf.coll_aabb_count);
    });
    sequence(h, &bsp.leafbrushes, |h, v| h.u16(*v));
    let tables = &mesh.tables;
    sequence(h, &tables.verts, floats);
    sequence(h, &tables.tri_indices, |h, v| h.u16(*v));
    h.bytes(&tables.tri_edge_is_walkable);
    sequence(h, &tables.tri_surface_flags, |h, v| h.u32(*v));
    sequence(h, &tables.tri_content_flags, |h, v| h.u32(*v));
    sequence(h, &tables.aabb_trees, |h, node| {
        floats(h, &node.origin);
        floats(h, &node.half_size);
        h.u16(node.material_index);
        h.u16(node.child_count);
        h.i32(node.u);
    });
    sequence(h, &tables.partitions, |h, part| {
        h.byte(part.tri_count);
        h.i32(part.first_tri);
        h.byte(part.first_vert_segment);
        h.byte(part.border_count);
        h.u32(part.first_border);
    });
    sequence(h, &tables.borders, |h, border| {
        floats(h, &border.dist_eq);
        h.f32(border.z_base);
        h.f32(border.z_slope);
        h.f32(border.start);
        h.f32(border.length);
    });
    sequence(h, &tables.aabb_roots, |h, v| h.u16(*v));
    sequence(h, &mesh.static_models, |h, model| {
        h.u32(model.index);
        h.bytes(model.name.as_bytes());
        let model = &model.model;
        floats(h, &model.origin);
        for axis in &model.inv_scaled_axis {
            floats(h, axis);
        }
        floats(h, &model.bounds_mid);
        floats(h, &model.bounds_half);
        h.u16(model.coll.coll_lod as u16);
        h.u32(model.coll.contents);
        sequence(h, &model.coll.surfs, |h, surface| {
            floats(h, &surface.midpoint);
            floats(h, &surface.half_size);
            h.i32(surface.bone_idx);
            h.u32(surface.contents);
            h.u32(surface.surf_flags);
            sequence(h, &surface.tris, |h, triangle| {
                floats(h, &triangle.plane);
                floats(h, &triangle.svec);
                floats(h, &triangle.tvec);
            });
        });
    });
    sequence(h, &models.models, |h, model| {
        floats(h, &model.mins);
        floats(h, &model.maxs);
        h.f32(model.radius);
        h.u32(model.first_brush);
        h.u16(model.num_brushes);
    });
    sequence(h, &models.triggers, |h, hulls| {
        sequence(h, hulls, |h, hull| {
            floats(h, &hull.mid);
            floats(h, &hull.half);
            sequence(h, &hull.slabs, |h, (axis, mid, half)| {
                floats(h, axis);
                h.f32(*mid);
                h.f32(*half);
            });
        });
    });
}

pub(crate) fn digest(content: &SimContent) -> u64 {
    let mut h = Digest::new();
    h.u64(super::CONTENT_DIGEST_SCHEME);
    clip(
        &mut h,
        content.clip_brushes(),
        content.clip_bsp(),
        content.clip_mesh(),
        content.clip_cmodels(),
    );
    let Some(atlas) = content.atlas() else {
        h.bool(false);
        return h.finish();
    };
    h.bool(true);
    match &atlas.space {
        asset_core::world_frame::SceneSpace::PortalAtlas => {
            h.byte(0);
            floats(
                &mut h,
                &asset_core::atlas::WorldLayout.origin(asset_core::atlas::WorldId(1)),
            );
        }
        asset_core::world_frame::SceneSpace::Shared(frames) => {
            h.byte(1);
            sequence(&mut h, frames, |h, frame| {
                floats(h, &frame.point_to_scene([0.0; 3]));
                for axis in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
                    floats(h, &frame.vector_to_scene(axis));
                }
            });
        }
    }
    h.bytes(atlas.root_zone.as_bytes());
    let versions = &atlas.versions;
    h.u64(versions.schema);
    h.u64(versions.aperture);
    h.u64(versions.trim_clearance);
    h.bytes(versions.generator_algorithm.as_bytes());
    h.u64(versions.generator_version);
    h.bool(versions.seed.is_some());
    if let Some(seed) = versions.seed {
        h.u64(seed);
    }
    h.bool(versions.template_digest.is_some());
    if let Some(digest) = versions.template_digest {
        h.u64(digest);
    }
    h.u64(atlas.dvars.len() as u64);
    for (key, value) in &atlas.dvars {
        h.bytes(key.as_bytes());
        h.bytes(value.as_bytes());
    }
    sequence(&mut h, atlas.worlds.get(1..).unwrap_or(&[]), |h, world| {
        h.bytes(world.zone.as_bytes());
        clip(h, &world.brushes, &world.bsp, &world.mesh, &world.cmodels);
    });
    sequence(&mut h, &atlas.portals, |h, portal| {
        for end in [&portal.a, &portal.b] {
            h.u64(end.world.index() as u64);
            h.bool(end.two_sided);
            floats(h, &end.base);
            floats(h, &end.normal);
            h.f32(end.width);
            h.f32(end.height);
        }
        h.bool(portal.flip);
    });
    sequence(&mut h, &atlas.portal_thickness, floats);
    sequence(&mut h, &atlas.portal_clearance, |h, pair| {
        for clearance in pair {
            h.byte(match clearance {
                asset_core::atlas::PortalClearance::Walk => 0,
                asset_core::atlas::PortalClearance::Flight => 1,
            });
        }
    });
    h.finish()
}
