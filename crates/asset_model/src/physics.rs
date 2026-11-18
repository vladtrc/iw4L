use bevy::prelude::Vec3;
use fastfile_iw4::{Ptr, XModelGeometry, ZonePtr, ZoneStream};
use xmodel_runtime::ModelMovementBrush;

fn offset(stream: &ZoneStream<'_>, at: Ptr, field: usize) -> Option<Ptr> {
    match stream.ptr_at(at, field).ok()? {
        ZonePtr::Offset(ptr) => Some(stream.resolve_alias(ptr)),
        _ => None,
    }
}

fn vector(stream: &ZoneStream<'_>, at: Ptr, field: usize) -> Option<Vec3> {
    let v = Vec3::new(
        stream.f32_at(at, field).ok()?,
        stream.f32_at(at, field + 4).ok()?,
        stream.f32_at(at, field + 8).ok()?,
    );
    v.is_finite().then_some(v)
}

fn bounds(stream: &ZoneStream<'_>, at: Ptr, field: usize) -> Option<(Vec3, Vec3)> {
    let mid = vector(stream, at, field)?;
    let half = vector(stream, at, field + 12)?;
    (half.min_element() > 0.0).then_some((mid, half))
}

fn box_planes(mid: Vec3, half: Vec3, axes: [Vec3; 3]) -> Vec<[f32; 4]> {
    axes.into_iter()
        .zip(half.to_array())
        .flat_map(|(axis, size)| {
            [-1.0, 1.0].map(|sign| {
                let normal = axis * sign;
                [normal.x, normal.y, normal.z, normal.dot(mid) + size]
            })
        })
        .collect()
}

fn shape_planes(stream: &ZoneStream<'_>, shape: Ptr) -> Option<Vec<[f32; 4]>> {
    match stream.i32_at(shape, stream.layout(4, 8)).ok()? {
        0 => {
            // Brush planes are already in model space. The six axial planes
            // are implicit in BrushWrapper::bounds, including for zero-sided brushes.
            let wrapper = offset(stream, shape, 0)?;
            let (mid, half) = bounds(stream, wrapper, 0)?;
            let mut planes = box_planes(mid, half, [Vec3::X, Vec3::Y, Vec3::Z]);
            let count = usize::from(stream.u16_at(wrapper, 24).ok()?);
            if count > 0 {
                let sides = offset(stream, wrapper, stream.layout(28, 32))?;
                for index in 0..count {
                    let side = sides.at(index * stream.layout(8, 16));
                    let plane = offset(stream, side, 0)?;
                    let normal = vector(stream, plane, 0)?;
                    let length = normal.length();
                    let distance = stream.f32_at(plane, 12).ok()?;
                    if length <= f32::EPSILON || !distance.is_finite() {
                        return None;
                    }
                    let normal = normal / length;
                    planes.push([normal.x, normal.y, normal.z, distance / length]);
                }
            }
            Some(planes)
        }
        1 => {
            let (mid, half) = bounds(stream, shape, stream.layout(44, 48))?;
            let base = stream.layout(8, 12);
            let axes = [
                vector(stream, shape, base)?,
                vector(stream, shape, base + 12)?,
                vector(stream, shape, base + 24)?,
            ];
            // Authored orientation is an orthonormal row basis, not Euler angles.
            if axes
                .iter()
                .any(|axis| (axis.length_squared() - 1.0).abs() > 0.001)
                || axes[0].dot(axes[1]).abs() > 0.001
                || axes[0].dot(axes[2]).abs() > 0.001
                || axes[1].dot(axes[2]).abs() > 0.001
            {
                return None;
            }
            Some(box_planes(mid, half, axes))
        }
        _ => None,
    }
}

pub(crate) fn capture_movement_brushes(
    stream: &ZoneStream<'_>,
    model: XModelGeometry,
) -> Vec<ModelMovementBrush> {
    let Some(map) = model.phys_collmap else {
        return Vec::new();
    };
    let Some(shapes) = map.geoms else {
        return Vec::new();
    };
    let name = map
        .name
        .and_then(|p| stream.cstr(p).ok())
        .unwrap_or("<unnamed>");
    (0..map.count)
        .filter_map(|index| {
            let shape = shapes.at(index * stream.layout(68, 72));
            let Some(planes) = shape_planes(stream, shape) else {
                let kind = stream.i32_at(shape, stream.layout(4, 8)).ok();
                diag::warn!(
                    Zone,
                    "PhysCollmap {name} shape {index} kind {kind:?}: movement geometry unavailable"
                );
                return None;
            };
            Some(ModelMovementBrush {
                plane_surface_flags: vec![0; planes.len()],
                planes,
                contents: model.contents,
            })
        })
        .collect()
}
