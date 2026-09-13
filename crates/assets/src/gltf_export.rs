use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::BufWriter,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use bevy::{
    prelude::{Mat3, Quat, Vec3},
    render::mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
};
use serde_json::{Map, Value, json};

use crate::{
    AuthoredMaterial, MaterialCatalog, MaterialCullFace, MaterialDrawMode, PreparedWorld,
    StaticModelPlacement, TS_COLOR_MAP,
};

const INCHES_TO_METERS: f32 = 0.0254;
type PlacementTrs = ([f32; 3], [f32; 4], [f32; 3]);

#[derive(Debug)]
pub struct GltfExportSummary {
    pub scene: PathBuf,
    pub primitives: u64,
    pub triangles: u64,
    pub placements: u64,
    pub materials: u64,
    pub images: u64,
    pub refusals: BTreeMap<&'static str, u64>,
    pub projections: BTreeMap<&'static str, u64>,
}

impl GltfExportSummary {
    pub fn report_line(&self) -> String {
        format!(
            "glTF P1a: primitives={} triangles={} placements={} materials={} images={} refusals={:?} projections={:?}",
            self.primitives,
            self.triangles,
            self.placements,
            self.materials,
            self.images,
            self.refusals,
            self.projections,
        )
    }
}

pub fn export_prepared_world_gltf(
    artifacts: &Path,
    map: &str,
    world: PreparedWorld,
) -> Result<GltfExportSummary, String> {
    let draw = world
        .draw
        .as_ref()
        .ok_or_else(|| "PreparedWorld has no GfxWorld draw product".to_owned())?;
    let export_id = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("system clock before unix epoch: {error}"))?
        .as_millis();
    let output = artifacts
        .join("exports")
        .join(safe_name(map))
        .join(format!("p1a-{export_id}"));

    let mut out = GltfBuilder::default();
    let world_node = out.add_world(draw)?;
    let mut model_meshes = Vec::with_capacity(world.static_model_meshes.len());
    for model in &world.static_model_meshes {
        model_meshes.push(out.add_model(model, draw));
    }

    let mut placement_nodes = Vec::new();
    for (slot, placement) in world.static_model_instances.iter().enumerate() {
        let Some(placement) = placement else {
            out.refuse("PlacementModelUnavailable");
            continue;
        };
        let Some(Some(mesh)) = model_meshes.get(placement.mesh) else {
            out.refuse("PlacementModelUnavailable");
            continue;
        };
        match placement_trs(placement) {
            Ok((translation, rotation, scale)) => {
                let model = &world.static_model_meshes[placement.mesh];
                let node = out.nodes.len();
                out.nodes.push(json!({
                    "name": format!("{}#slot-{slot:06}", model.name),
                    "mesh": mesh,
                    "translation": translation,
                    "rotation": rotation,
                    "scale": scale,
                    "extras": {"iw4l": {
                        "authored_slot": slot,
                        "model": model.name,
                        "cull_distance": placement.cull_dist,
                        "reflection_probe_index": placement.reflection_probe_index,
                        "primary_light_index": placement.primary_light_index,
                        "flags": placement.flags,
                    }},
                }));
                placement_nodes.push(node);
                out.placements += 1;
            }
            Err(reason) => out.refuse(reason),
        }
    }

    let static_group = out.nodes.len();
    out.nodes.push(json!({
        "name": "StaticModels",
        "children": placement_nodes,
    }));
    let root = out.nodes.len();
    out.nodes.push(json!({
        "name": map,
        "children": [world_node, static_group],
    }));
    if out.primitives == 0 {
        return Err(format!(
            "no glTF primitive survived P1a policy; refusals={:?}",
            out.refusals
        ));
    }

    let document = json!({
        "asset": {"version": "2.0", "generator": "iw4l P1a semantic exporter"},
        "scene": 0,
        "scenes": [{"name": map, "nodes": [root]}],
        "nodes": out.nodes,
        "meshes": out.meshes,
        "materials": out.materials,
        "textures": out.textures,
        "images": out.images,
        "samplers": out.samplers,
        "accessors": out.accessors,
        "bufferViews": out.buffer_views,
        "buffers": [{"uri": "scene.bin", "byteLength": out.bin.len()}],
    });

    fs::create_dir_all(output.join("textures"))
        .map_err(|error| format!("create {}: {error}", output.display()))?;
    fs::write(output.join("scene.bin"), &out.bin)
        .map_err(|error| format!("write scene.bin: {error}"))?;
    for image in &out.pending_images {
        write_png(&output.join("textures").join(&image.file_name), image)?;
    }
    let gltf = serde_json::to_vec_pretty(&document)
        .map_err(|error| format!("serialize scene.gltf: {error}"))?;
    let scene = output.join("scene.gltf");
    fs::write(&scene, gltf).map_err(|error| format!("write {}: {error}", scene.display()))?;

    Ok(GltfExportSummary {
        scene,
        primitives: out.primitives,
        triangles: out.triangles,
        placements: out.placements,
        materials: out.materials.len() as u64,
        images: out.images.len() as u64,
        refusals: out.refusals,
        projections: out.projections,
    })
}

#[derive(Default)]
struct GltfBuilder {
    bin: Vec<u8>,
    buffer_views: Vec<Value>,
    accessors: Vec<Value>,
    meshes: Vec<Value>,
    nodes: Vec<Value>,
    materials: Vec<Value>,
    textures: Vec<Value>,
    images: Vec<Value>,
    samplers: Vec<Value>,
    pending_images: Vec<PendingImage>,
    material_cache: HashMap<usize, Result<usize, &'static str>>,
    texture_cache: HashMap<(usize, u8), usize>,
    image_cache: HashMap<usize, usize>,
    sampler_cache: HashMap<u8, usize>,
    refusals: BTreeMap<&'static str, u64>,
    projections: BTreeMap<&'static str, u64>,
    primitives: u64,
    triangles: u64,
    placements: u64,
}

impl GltfBuilder {
    fn add_world(&mut self, draw: &crate::WorldDraw) -> Result<usize, String> {
        let mut primitives = Vec::new();
        for batch in &draw.batches {
            let Some(material) = canonical_material(batch.material, &draw.material_asset_ids)
            else {
                self.refuse("MaterialUnbound");
                continue;
            };
            let material = match self.material(material, &draw.global_materials) {
                Ok(material) => material,
                Err(reason) => {
                    self.refuse(reason);
                    continue;
                }
            };
            match self.primitive(&batch.mesh, material) {
                Ok(primitive) => primitives.push(primitive),
                Err(reason) => self.refuse(reason),
            }
        }
        if primitives.is_empty() {
            return Err(format!(
                "world has no P1a-exportable primitive; refusals={:?}",
                self.refusals
            ));
        }
        let mesh = self.meshes.len();
        self.meshes
            .push(json!({"name": "World", "primitives": primitives}));
        let node = self.nodes.len();
        self.nodes.push(json!({"name": "World", "mesh": mesh}));
        Ok(node)
    }

    fn add_model(&mut self, model: &crate::ModelMesh, draw: &crate::WorldDraw) -> Option<usize> {
        let mut primitives = Vec::new();
        for surface in model.surfaces() {
            let Some(material) = canonical_material(surface.material, &draw.material_asset_ids)
            else {
                self.refuse("MaterialUnbound");
                continue;
            };
            let material = match self.material(material, &draw.global_materials) {
                Ok(material) => material,
                Err(reason) => {
                    self.refuse(reason);
                    continue;
                }
            };
            match self.primitive(&surface.mesh, material) {
                Ok(primitive) => primitives.push(primitive),
                Err(reason) => self.refuse(reason),
            }
        }
        if primitives.is_empty() {
            return None;
        }
        let mesh = self.meshes.len();
        self.meshes
            .push(json!({"name": model.name, "primitives": primitives}));
        Some(mesh)
    }

    fn primitive(
        &mut self,
        mesh: &bevy::prelude::Mesh,
        material: usize,
    ) -> Result<Value, &'static str> {
        if mesh.primitive_topology() != PrimitiveTopology::TriangleList {
            return Err("GeometryTopologyUnsupported");
        }
        let positions = required_f32x3(mesh.attribute(bevy::prelude::Mesh::ATTRIBUTE_POSITION))?;
        if positions.is_empty() {
            return Err("GeometryAttributeMissing");
        }
        let indices = mesh.indices().ok_or("GeometryIndexMissing")?;
        if indices.is_empty() || indices.len() % 3 != 0 {
            return Err("GeometryIndexInvalid");
        }
        if indices.iter().any(|index| index >= positions.len()) {
            return Err("GeometryIndexOutOfRange");
        }

        let positions = positions
            .iter()
            .copied()
            .map(convert_position)
            .collect::<Vec<_>>();
        if !positions.iter().flatten().all(|value| value.is_finite()) {
            return Err("GeometryNonFinite");
        }
        let mut attributes = Map::new();
        attributes.insert(
            "POSITION".into(),
            json!(self.vec3_accessor(&positions, true)),
        );

        if let Some(values) = optional_f32x3(mesh.attribute(bevy::prelude::Mesh::ATTRIBUTE_NORMAL))?
        {
            require_count(values.len(), positions.len())?;
            let mut converted = Vec::with_capacity(values.len());
            for &value in values {
                converted.push(convert_direction(value)?);
            }
            attributes.insert(
                "NORMAL".into(),
                json!(self.vec3_accessor(&converted, false)),
            );
        }
        if let Some(values) =
            optional_f32x4(mesh.attribute(bevy::prelude::Mesh::ATTRIBUTE_TANGENT))?
        {
            require_count(values.len(), positions.len())?;
            let mut converted = Vec::with_capacity(values.len());
            for &[x, y, z, w] in values {
                if !w.is_finite() {
                    return Err("GeometryNonFinite");
                }
                let [x, y, z] = convert_direction([x, y, z])?;
                converted.push([x, y, z, w]);
            }
            attributes.insert("TANGENT".into(), json!(self.vec4_accessor(&converted)));
        }
        if let Some(values) = optional_f32x2(mesh.attribute(bevy::prelude::Mesh::ATTRIBUTE_UV_0))? {
            require_count(values.len(), positions.len())?;
            if !values.iter().flatten().all(|value| value.is_finite()) {
                return Err("GeometryNonFinite");
            }
            attributes.insert("TEXCOORD_0".into(), json!(self.vec2_accessor(values)));
        }
        if let Some(values) = optional_f32x4(mesh.attribute(bevy::prelude::Mesh::ATTRIBUTE_COLOR))?
        {
            require_count(values.len(), positions.len())?;
            if !values.iter().flatten().all(|value| value.is_finite()) {
                return Err("GeometryNonFinite");
            }
            attributes.insert("COLOR_0".into(), json!(self.vec4_accessor(values)));
        }

        let index_values = match indices {
            Indices::U16(values) => values.iter().map(|&value| u32::from(value)).collect(),
            Indices::U32(values) => values.clone(),
        };
        let index_accessor = self.index_accessor(&index_values);
        self.primitives += 1;
        self.triangles += (index_values.len() / 3) as u64;
        Ok(json!({
            "attributes": attributes,
            "indices": index_accessor,
            "material": material,
            "mode": 4,
        }))
    }

    fn material(
        &mut self,
        canonical: usize,
        catalog: &MaterialCatalog,
    ) -> Result<usize, &'static str> {
        if let Some(cached) = self.material_cache.get(&canonical) {
            return *cached;
        }
        let result = self.build_material(canonical, catalog);
        self.material_cache.insert(canonical, result);
        result
    }

    fn build_material(
        &mut self,
        canonical: usize,
        catalog: &MaterialCatalog,
    ) -> Result<usize, &'static str> {
        let material = catalog.materials.get(canonical).ok_or("MaterialUnbound")?;
        let (alpha_mode, alpha_cutoff, coverage_exact) = match catalog.agreed_draw_mode(material) {
            Some(MaterialDrawMode::Opaque) => ("OPAQUE", None, true),
            Some(MaterialDrawMode::AlphaTest { .. }) => (
                "MASK",
                Some(
                    catalog
                        .agreed_alpha_test_cutoff(material)
                        .flatten()
                        .ok_or("MaterialStateDisagreement")?,
                ),
                true,
            ),
            Some(_) => return Err("MaterialBlendUnsupported"),
            None => match catalog.agreed_alpha_test_cutoff(material) {
                Some(Some(cutoff)) => ("MASK", Some(cutoff), false),
                _ => ("OPAQUE", None, false),
            },
        };
        let (double_sided, cull_exact) = match catalog.cull_face(material) {
            Some(MaterialCullFace::Back) => (false, true),
            Some(MaterialCullFace::None) => (true, true),
            Some(MaterialCullFace::Front) => return Err("MaterialCullFrontUnsupported"),
            None => (true, false),
        };
        let binding = single_color_binding(material)?;
        let image = binding.image.ok_or("MaterialImageUnavailable")?;
        let texture = self.texture(image, binding.sampler_state, catalog)?;
        let transform = format!("{:?}", catalog.color_map_transform(material)).to_lowercase();
        self.project("MaterialPbrPreview");
        if !coverage_exact {
            self.project("MaterialCoverageOpaquePreview");
        }
        if !cull_exact {
            self.project("MaterialDoubleSidedPreview");
        }
        if binding.sampler_state & 0b111 >= 3 {
            self.project("SamplerAnisotropyUnrepresented");
        }

        let index = self.materials.len();
        let mut value = json!({
            "name": material.name.as_str(),
            "pbrMetallicRoughness": {
                "baseColorTexture": {"index": texture},
                "metallicFactor": 0.0,
                "roughnessFactor": 1.0,
            },
            "alphaMode": alpha_mode,
            "doubleSided": double_sided,
            "extras": {"iw4l": {
                "source_catalog_index": canonical,
                "source_name": material.name.as_str(),
                "technique_set": material.technique_set.as_str(),
                "draw_mode": format!("{:?}", catalog.agreed_draw_mode(material)),
                "cull": format!("{:?}", catalog.cull_face(material)),
                "color_transform": transform,
                "sampler_state": binding.sampler_state,
                "coverage_exact": coverage_exact,
                "cull_exact": cull_exact,
                "exact": false,
                "projection": "P1a base-colour preview; IW shader response is not represented",
            }},
        });
        if let Some(cutoff) = alpha_cutoff {
            value["alphaCutoff"] = json!(cutoff);
        }
        self.materials.push(value);
        Ok(index)
    }

    fn texture(
        &mut self,
        image_index: usize,
        sampler_state: u8,
        catalog: &MaterialCatalog,
    ) -> Result<usize, &'static str> {
        if let Some(&texture) = self.texture_cache.get(&(image_index, sampler_state)) {
            return Ok(texture);
        }
        let source = catalog
            .images
            .get(image_index)
            .ok_or("MaterialImageUnavailable")?;
        let decoded = source.decoded.as_ref().ok_or("MaterialImageUnavailable")?;
        let image = if let Some(&image) = self.image_cache.get(&image_index) {
            image
        } else {
            let (width, height, pixels) = asset_material::decoded_image_top_level_rgba8(decoded)
                .map_err(|_| "MaterialImageUnsupported")?;
            let file_name = format!("{}-{image_index:06}.png", safe_name(source.name.as_str()));
            let image = self.images.len();
            self.images.push(json!({
                "name": source.name.as_str(),
                "uri": format!("textures/{file_name}"),
            }));
            self.pending_images.push(PendingImage {
                file_name,
                width,
                height,
                pixels,
            });
            self.image_cache.insert(image_index, image);
            image
        };
        let sampler = if let Some(&sampler) = self.sampler_cache.get(&sampler_state) {
            sampler
        } else {
            let sampler = self.samplers.len();
            self.samplers.push(sampler_json(sampler_state)?);
            self.sampler_cache.insert(sampler_state, sampler);
            sampler
        };
        let texture = self.textures.len();
        self.textures.push(json!({
            "name": source.name.as_str(),
            "source": image,
            "sampler": sampler,
        }));
        self.texture_cache
            .insert((image_index, sampler_state), texture);
        Ok(texture)
    }

    fn vec2_accessor(&mut self, values: &[[f32; 2]]) -> usize {
        self.float_accessor(values.iter().flatten().copied(), values.len(), "VEC2", None)
    }

    fn vec3_accessor(&mut self, values: &[[f32; 3]], bounds: bool) -> usize {
        let min_max = bounds.then(|| vec3_bounds(values));
        self.float_accessor(
            values.iter().flatten().copied(),
            values.len(),
            "VEC3",
            min_max,
        )
    }

    fn vec4_accessor(&mut self, values: &[[f32; 4]]) -> usize {
        self.float_accessor(values.iter().flatten().copied(), values.len(), "VEC4", None)
    }

    fn float_accessor(
        &mut self,
        values: impl Iterator<Item = f32>,
        count: usize,
        kind: &'static str,
        min_max: Option<([f32; 3], [f32; 3])>,
    ) -> usize {
        self.align_bin();
        let offset = self.bin.len();
        for value in values {
            self.bin.extend_from_slice(&value.to_le_bytes());
        }
        let view = self.push_view(offset, self.bin.len() - offset, 34962);
        let accessor = self.accessors.len();
        let mut value = json!({
            "bufferView": view,
            "componentType": 5126,
            "count": count,
            "type": kind,
        });
        if let Some((min, max)) = min_max {
            value["min"] = json!(min);
            value["max"] = json!(max);
        }
        self.accessors.push(value);
        accessor
    }

    fn index_accessor(&mut self, values: &[u32]) -> usize {
        let min = values
            .iter()
            .copied()
            .min()
            .expect("primitive rejects empty index buffers");
        let max = values
            .iter()
            .copied()
            .max()
            .expect("primitive rejects empty index buffers");
        self.align_bin();
        let offset = self.bin.len();
        for &value in values {
            self.bin.extend_from_slice(&value.to_le_bytes());
        }
        let view = self.push_view(offset, self.bin.len() - offset, 34963);
        let accessor = self.accessors.len();
        self.accessors.push(json!({
            "bufferView": view,
            "componentType": 5125,
            "count": values.len(),
            "type": "SCALAR",
            "min": [min],
            "max": [max],
        }));
        accessor
    }

    fn align_bin(&mut self) {
        while !self.bin.len().is_multiple_of(4) {
            self.bin.push(0);
        }
    }

    fn push_view(&mut self, offset: usize, len: usize, target: u32) -> usize {
        let index = self.buffer_views.len();
        self.buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": offset,
            "byteLength": len,
            "target": target,
        }));
        index
    }

    fn refuse(&mut self, reason: &'static str) {
        *self.refusals.entry(reason).or_default() += 1;
    }

    fn project(&mut self, reason: &'static str) {
        *self.projections.entry(reason).or_default() += 1;
    }
}

struct PendingImage {
    file_name: String,
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

fn canonical_material(local: Option<usize>, remap: &[Option<usize>]) -> Option<usize> {
    remap.get(local?).copied().flatten()
}

fn single_color_binding(
    material: &AuthoredMaterial,
) -> Result<&crate::MaterialTextureBinding, &'static str> {
    let mut bindings = material
        .textures
        .iter()
        .filter(|binding| binding.semantic == TS_COLOR_MAP);
    let binding = bindings.next().ok_or("MaterialColorMapUnavailable")?;
    if bindings.next().is_some() {
        return Err("MaterialColorMapAmbiguous");
    }
    Ok(binding)
}

fn sampler_json(state: u8) -> Result<Value, &'static str> {
    let linear = match state & 0b111 {
        1 => false,
        2..=4 => true,
        _ => return Err("MaterialSamplerUnsupported"),
    };
    let mip = (state >> 3) & 0b11;
    let mag = if linear { 9729 } else { 9728 };
    let min = match (linear, mip) {
        (false, 0) => 9728,
        (true, 0) => 9729,
        (false, 1) => 9984,
        (true, 1) => 9985,
        (false, 2) => 9986,
        (true, 2) => 9987,
        (_, _) => return Err("MaterialSamplerUnsupported"),
    };
    Ok(json!({
        "magFilter": mag,
        "minFilter": min,
        "wrapS": if state & (1 << 5) != 0 { 33071 } else { 10497 },
        "wrapT": if state & (1 << 6) != 0 { 33071 } else { 10497 },
        "extras": {"iw4l": {"sampler_state": state}},
    }))
}

fn placement_trs(placement: &StaticModelPlacement) -> Result<PlacementTrs, &'static str> {
    let columns = placement.axis.map(Vec3::from_array);
    if !placement.origin.iter().all(|value| value.is_finite())
        || !placement.scale.is_finite()
        || placement.scale == 0.0
        || columns.iter().any(|axis| !axis.is_finite())
    {
        return Err("PlacementTransformInvalid");
    }
    const TOLERANCE: f32 = 1.0e-3;
    if columns
        .iter()
        .any(|axis| (axis.length_squared() - 1.0).abs() > TOLERANCE)
        || columns[0].dot(columns[1]).abs() > TOLERANCE
        || columns[0].dot(columns[2]).abs() > TOLERANCE
        || columns[1].dot(columns[2]).abs() > TOLERANCE
    {
        return Err("PlacementTransformInvalid");
    }
    let source = Mat3::from_cols(columns[0], columns[1], columns[2]);
    if (source.determinant() - 1.0).abs() > TOLERANCE {
        return Err("PlacementTransformInvalid");
    }
    let conversion = basis_conversion();
    let converted = conversion * source * conversion.transpose();
    let rotation = Quat::from_mat3(&converted).normalize();
    if !rotation.is_finite() {
        return Err("PlacementTransformInvalid");
    }
    Ok((
        convert_position(placement.origin),
        rotation.to_array(),
        [placement.scale; 3],
    ))
}

fn basis_conversion() -> Mat3 {
    Mat3::from_cols(Vec3::X, -Vec3::Z, Vec3::Y)
}

fn convert_position([x, y, z]: [f32; 3]) -> [f32; 3] {
    [
        x * INCHES_TO_METERS,
        z * INCHES_TO_METERS,
        -y * INCHES_TO_METERS,
    ]
}

fn convert_direction([x, y, z]: [f32; 3]) -> Result<[f32; 3], &'static str> {
    let value = Vec3::new(x, z, -y);
    if !value.is_finite() || value.length_squared() <= f32::EPSILON {
        return Err("GeometryNonFinite");
    }
    Ok(value.normalize().to_array())
}

fn vec3_bounds(values: &[[f32; 3]]) -> ([f32; 3], [f32; 3]) {
    let mut min = [f32::INFINITY; 3];
    let mut max = [f32::NEG_INFINITY; 3];
    for value in values {
        for component in 0..3 {
            min[component] = min[component].min(value[component]);
            max[component] = max[component].max(value[component]);
        }
    }
    (min, max)
}

fn required_f32x3(values: Option<&VertexAttributeValues>) -> Result<&[[f32; 3]], &'static str> {
    optional_f32x3(values)?.ok_or("GeometryAttributeMissing")
}

fn optional_f32x2(
    values: Option<&VertexAttributeValues>,
) -> Result<Option<&[[f32; 2]]>, &'static str> {
    match values {
        Some(VertexAttributeValues::Float32x2(values)) => Ok(Some(values)),
        Some(_) => Err("GeometryAttributeFormatUnsupported"),
        None => Ok(None),
    }
}

fn optional_f32x3(
    values: Option<&VertexAttributeValues>,
) -> Result<Option<&[[f32; 3]]>, &'static str> {
    match values {
        Some(VertexAttributeValues::Float32x3(values)) => Ok(Some(values)),
        Some(_) => Err("GeometryAttributeFormatUnsupported"),
        None => Ok(None),
    }
}

fn optional_f32x4(
    values: Option<&VertexAttributeValues>,
) -> Result<Option<&[[f32; 4]]>, &'static str> {
    match values {
        Some(VertexAttributeValues::Float32x4(values)) => Ok(Some(values)),
        Some(_) => Err("GeometryAttributeFormatUnsupported"),
        None => Ok(None),
    }
}

fn require_count(got: usize, expected: usize) -> Result<(), &'static str> {
    if got == expected {
        Ok(())
    } else {
        Err("GeometryAttributeCountMismatch")
    }
}

fn safe_name(name: &str) -> String {
    let safe = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if safe.is_empty() {
        "unnamed".into()
    } else {
        safe
    }
}

fn write_png(path: &Path, image: &PendingImage) -> Result<(), String> {
    let file =
        fs::File::create(path).map_err(|error| format!("create {}: {error}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|error| format!("PNG header {}: {error}", path.display()))?;
    writer
        .write_image_data(&image.pixels)
        .map_err(|error| format!("PNG pixels {}: {error}", path.display()))
}
