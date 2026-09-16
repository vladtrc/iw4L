use std::collections::HashSet;
use std::sync::Arc;

use bevy::prelude::*;

use crate::assemble::drawsurf::RuntimeLightmapHandles;

use super::world::WorldScene;

pub(crate) const IMAGE_BYTES_PER_OVERLAY_FRAME: u64 = 32 * 1024 * 1024;
pub(crate) const IMAGES_PER_OVERLAY_FRAME: u32 = 16;

pub(crate) fn overlay_count_byte_capped(
    count: u32,
    bytes: u64,
    max_count: u32,
    byte_budget: u64,
) -> bool {
    count >= max_count && bytes >= byte_budget
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbeDebug {
    Flat([u8; 4]),

    Faces,

    Mips { shift: usize },
}

const PROBE_FACE_COLOURS: [[u8; 4]; 6] = [
    [255, 0, 0, 255],
    [0, 255, 255, 255],
    [0, 255, 0, 255],
    [255, 0, 255, 255],
    [0, 0, 255, 255],
    [255, 255, 0, 255],
];

const PROBE_MIP_COLOURS: [[u8; 4]; 8] = [
    [255, 0, 0, 255],
    [255, 128, 0, 255],
    [255, 255, 0, 255],
    [0, 255, 0, 255],
    [0, 255, 255, 255],
    [0, 0, 255, 255],
    [255, 0, 255, 255],
    [255, 255, 255, 255],
];

fn probe_debug_mode() -> Option<ProbeDebug> {
    use std::sync::OnceLock;
    static MODE: OnceLock<Option<ProbeDebug>> = OnceLock::new();
    *MODE.get_or_init(|| {
        let raw = std::env::var("IW4L_PROBE_DEBUG").ok()?;
        match raw.trim() {
            "faces" => {
                diag::warn!(World, "reflection probe A/B: every cube layer flat, +X red -X cyan +Y green -Y magenta +Z blue -Z yellow");
                return Some(ProbeDebug::Faces);
            }
            "mips" => {
                diag::warn!(World, "reflection probe A/B: every mip flat, 0 red 1 orange 2 yellow 3 green 4 cyan 5 blue 6 magenta 7 white");
                return Some(ProbeDebug::Mips { shift: 0 });
            }
            "mips-shift" => {
                diag::warn!(World, "reflection probe A/B: every mip flat, palette rotated by one level (0 white 1 red 2 orange 3 yellow 4 green 5 cyan 6 blue 7 magenta)");
                return Some(ProbeDebug::Mips { shift: 1 });
            }
            _ => {}
        }
        let mut parts = raw.split(',').map(|part| part.trim().parse::<u8>());
        let (Some(Ok(r)), Some(Ok(g)), Some(Ok(b))) = (parts.next(), parts.next(), parts.next())
        else {
            diag::warn!(World, "unparseable IW4L_PROBE_DEBUG={raw:?}; probes unchanged");
            return None;
        };
        diag::warn!(World, "reflection probe A/B: every probe forced to {r},{g},{b},255");
        Some(ProbeDebug::Flat([r, g, b, 255]))
    })
}

fn paint_probe_debug(image: &mut Image, mode: ProbeDebug) {
    let size = image.texture_descriptor.size.width;
    let levels = image.texture_descriptor.mip_level_count;
    let Some(data) = image.data.as_mut() else {
        diag::warn!(
            World,
            "IW4L_PROBE_DEBUG: probe cube has no CPU data; left unpainted"
        );
        return;
    };
    let expected: usize = (0..levels)
        .map(|level| {
            let side = (size >> level).max(1) as usize;
            side * side * 4 * 6
        })
        .sum();
    if data.len() != expected {
        diag::warn!(
            World,
            "IW4L_PROBE_DEBUG: probe cube is {} bytes, mip walk of {size}px x{levels} needs {expected}; left unpainted",
            data.len()
        );
        return;
    }
    let mut offset = 0usize;
    for level in 0..levels {
        let side = (size >> level).max(1) as usize;
        let level_bytes = side * side * 4;
        for face in 0..6usize {
            let colour = match mode {
                ProbeDebug::Flat(colour) => colour,
                ProbeDebug::Faces => PROBE_FACE_COLOURS[face],

                ProbeDebug::Mips { shift } if (level as usize) < PROBE_MIP_COLOURS.len() => {
                    let len = PROBE_MIP_COLOURS.len();
                    PROBE_MIP_COLOURS[(level as usize + len - shift % len) % len]
                }
                ProbeDebug::Mips { .. } => [128, 128, 128, 255],
            };
            for texel in data[offset..offset + level_bytes].chunks_exact_mut(4) {
                texel.copy_from_slice(&colour);
            }
            offset += level_bytes;
        }
    }
}

fn image_bytes(image: &Image) -> u64 {
    image
        .data
        .as_ref()
        .map(|data| data.len() as u64)
        .unwrap_or(0)
}

#[derive(Default)]
pub struct WorldImageUpload {
    pub done: u32,
    pub total: u32,
    /// Bytes handed to `Assets<Image>`. **Not** bytes on the GPU: the asset
    /// server takes the image here, the render world turns it into a texture
    /// some frames later, and the driver copies it later still. Calling this
    /// "uploaded" is how a handoff rate gets reported as a transfer rate.
    pub handed_bytes: u64,
    pub bytes_total: u64,
    /// The longest single handoff, and how big it was. A frame budget can only
    /// be kept to the granularity of the largest thing that cannot be split —
    /// so when a 40 ms slice runs to 227 ms, this is the number that says
    /// whether the budget was ignored or was never keepable.
    pub largest_step_ns: u64,
    pub largest_step_bytes: u64,
    pub handoff_ns: u64,
    pub steps: u64,

    pub skipped: u32,

    pub pipeline_world_materials: Arc<std::collections::HashSet<u16>>,

    pub pipeline_smodel_materials: Arc<std::collections::HashSet<u16>>,
    exact_images: Vec<Option<Image>>,
    exact_variants: Vec<Option<assets::ImageVariantId>>,
    /// One asset per prepared variant. Two catalog slots that hold the same
    /// variant hold byte-identical images with the same sampler and colour
    /// space; giving each its own `Assets<Image>` entry uploads the same
    /// texture twice and keeps two copies of it resident, so the second slot
    /// takes a clone of the first one's handle and drops its own copy.
    exact_by_variant: std::collections::HashMap<assets::ImageVariantId, Handle<Image>>,
    /// Slots that took another slot's handle, and the bytes that saved.
    pub reused_handles: u32,
    pub reused_handle_bytes: u64,
    exact_handles: Vec<Option<Handle<Image>>>,
    exact_at: usize,
    probes: Vec<Option<Image>>,
    probe_handles: Vec<Option<Handle<Image>>>,
    probe_at: usize,
    lightmaps: Vec<Option<super::world::WorldLightmap>>,
    lightmap_handles: Vec<Option<RuntimeLightmapHandles>>,
    lightmap_at: usize,
    upload_stage: Option<assets::LoadStage>,
    reachable_exact: HashSet<u32>,
}

impl WorldImageUpload {
    pub fn unfinished(&self) -> bool {
        self.done < self.total
    }

    pub fn tess_handles(
        &self,
    ) -> (
        &[Option<Handle<Image>>],
        &[Option<RuntimeLightmapHandles>],
        &[Option<Handle<Image>>],
    ) {
        (
            &self.exact_handles,
            &self.lightmap_handles,
            &self.probe_handles,
        )
    }

    pub fn exact_handles(&self) -> &[Option<Handle<Image>>] {
        &self.exact_handles
    }

    pub fn probe_handles(&self) -> &[Option<Handle<Image>>] {
        &self.probe_handles
    }

    pub fn lightmap_handles(&self) -> &[Option<RuntimeLightmapHandles>] {
        &self.lightmap_handles
    }

    pub fn take_stage(&mut self) -> Option<assets::LoadStage> {
        self.upload_stage.take()
    }

    pub fn arm(&mut self, scene: &mut WorldScene, progress: Option<&assets::LoadProgress>) {
        self.pipeline_world_materials = Arc::new(
            scene
                .batches
                .iter()
                .filter_map(|batch| batch.material)
                .filter_map(|id| {
                    scene
                        .runtime_material_catalog
                        .derived(id)
                        .map(|material| material.asset_id.0)
                })
                .collect(),
        );
        self.pipeline_smodel_materials = Arc::new(
            scene
                .static_model_meshes
                .iter()
                .chain(scene.sky_model.iter())
                .flat_map(|mesh| mesh.surface_materials())
                .flatten()
                .filter_map(|id| {
                    scene
                        .runtime_material_catalog
                        .derived(id)
                        .map(|material| material.asset_id.0)
                })
                .collect(),
        );
        self.exact_images = std::mem::take(&mut scene.exact_material_images);
        self.exact_variants = std::mem::take(&mut scene.exact_material_variants);
        self.exact_variants.resize(self.exact_images.len(), None);
        self.exact_by_variant.clear();
        self.reused_handles = 0;
        self.reused_handle_bytes = 0;
        self.exact_handles = vec![None; self.exact_images.len()];
        self.reachable_exact = reachable_exact_image_slots(
            &scene.runtime_material_catalog,
            &scene.exact_material_names,
        );

        self.reachable_exact.extend(
            scene
                .primary_light_attenuation
                .iter()
                .filter_map(|light| light.image),
        );
        self.reachable_exact.extend(scene.outdoor_image);
        if let Some(sun) = scene.sun_effects {
            self.reachable_exact.extend(sun.sprite_image);
            self.reachable_exact.extend(sun.flare_image);
            self.pipeline_world_materials = Arc::new({
                let mut set = (*self.pipeline_world_materials).clone();
                for material in [sun.sprite_material, sun.flare_material]
                    .into_iter()
                    .flatten()
                {
                    if let Ok(id) = u16::try_from(material) {
                        set.insert(id);
                    }
                }
                set
            });
        }
        self.probes = std::mem::take(&mut scene.reflection_probes);
        self.probe_handles = vec![None; self.probes.len()];
        self.lightmaps = std::mem::take(&mut scene.lightmaps);
        self.lightmap_handles = vec![None; self.lightmaps.len()];
        self.total = (self.exact_images.len()
            + self.probes.len()
            + self.lightmaps.iter().filter(|page| page.is_some()).count())
            as u32;
        self.done = 0;
        self.skipped = 0;
        self.handed_bytes = 0;
        self.bytes_total = self
            .exact_images
            .iter()
            .flatten()
            .map(image_bytes)
            .sum::<u64>()
            + self.probes.iter().flatten().map(image_bytes).sum::<u64>();
        self.exact_at = 0;
        self.probe_at = 0;
        self.lightmap_at = 0;
        if let Some(progress) = progress {
            let stage = progress.stage("uploading world images");
            stage.total(u64::from(self.total));
            self.upload_stage = Some(stage);
        }
    }

    /// One handoff that could not be interrupted. Kept as a maximum rather
    /// than a histogram: the question is what the smallest keepable budget is,
    /// and that is the largest step, not its distribution.
    fn note_step(&mut self, took: std::time::Duration, bytes: u64) {
        let ns = u64::try_from(took.as_nanos()).unwrap_or(u64::MAX);
        self.steps = self.steps.saturating_add(1);
        self.handoff_ns = self.handoff_ns.saturating_add(ns);
        if ns > self.largest_step_ns {
            self.largest_step_ns = ns;
            self.largest_step_bytes = bytes;
        }
    }

    pub fn until(
        &mut self,
        images: &mut Assets<Image>,
        deadline: Option<std::time::Instant>,
        max_this_frame: u32,
    ) -> bool {
        let start_done = self.done;
        let start_bytes = self.handed_bytes;
        let byte_budget = if max_this_frame == u32::MAX {
            u64::MAX
        } else {
            IMAGE_BYTES_PER_OVERLAY_FRAME
        };
        let capped = |done: u32, bytes: u64| {
            deadline.is_some_and(|end| std::time::Instant::now() >= end)
                || overlay_count_byte_capped(
                    done.saturating_sub(start_done),
                    bytes.saturating_sub(start_bytes),
                    max_this_frame,
                    byte_budget,
                )
        };
        let mut stepped = false;
        while self.exact_at < self.exact_images.len() {
            if stepped && capped(self.done, self.handed_bytes) {
                return false;
            }
            let image = self.exact_images[self.exact_at].take();
            let slot = self.exact_at as u32;
            if !self.reachable_exact.is_empty() && !self.reachable_exact.contains(&slot) {
                self.exact_at += 1;
                self.done = self.done.saturating_add(1);
                self.skipped = self.skipped.saturating_add(1);
                stepped = true;
                self.sync_stage();
                continue;
            }
            let bytes = image.as_ref().map(image_bytes).unwrap_or(0);
            self.handed_bytes += bytes;
            let step = std::time::Instant::now();
            let variant = self.exact_variants[self.exact_at];
            self.exact_handles[self.exact_at] = image.map(|image| {
                // A slot whose variant is already an asset takes that handle
                // and lets its own copy go: the two are the same texels under
                // the same sampler, and a second `add` is a second texture.
                let Some(variant) = variant else {
                    return images.add(image);
                };
                if let Some(handle) = self.exact_by_variant.get(&variant) {
                    self.reused_handles = self.reused_handles.saturating_add(1);
                    self.reused_handle_bytes = self.reused_handle_bytes.saturating_add(bytes);
                    return handle.clone();
                }
                let handle = images.add(image);
                self.exact_by_variant.insert(variant, handle.clone());
                handle
            });
            self.note_step(step.elapsed(), bytes);
            self.exact_at += 1;
            self.done = self.done.saturating_add(1);
            stepped = true;
            self.sync_stage();
        }
        while self.probe_at < self.probes.len() {
            if stepped && capped(self.done, self.handed_bytes) {
                return false;
            }
            let image = self.probes[self.probe_at].take();
            let bytes = image.as_ref().map(image_bytes).unwrap_or(0);
            self.handed_bytes += bytes;
            let step = std::time::Instant::now();
            self.probe_handles[self.probe_at] = image.map(|mut image| {
                if let Some(mode) = probe_debug_mode() {
                    paint_probe_debug(&mut image, mode);
                }
                images.add(image)
            });
            self.note_step(step.elapsed(), bytes);
            self.probe_at += 1;
            self.done = self.done.saturating_add(1);
            stepped = true;
            self.sync_stage();
        }
        while self.lightmap_at < self.lightmaps.len() {
            if stepped && capped(self.done, self.handed_bytes) {
                return false;
            }
            let page = self.lightmaps[self.lightmap_at].take();
            let step = std::time::Instant::now();
            self.lightmap_handles[self.lightmap_at] = page.map(|lightmap| {
                diag::info!(
                    World,
                    "world lightmap[{}]: secondary={} 2x{}x{} mask={} {}x{}",
                    self.lightmap_at,
                    lightmap.ambient_source_name,
                    lightmap.ambient_size.x,
                    lightmap.ambient_size.y,
                    lightmap.sun_mask_source_name,
                    lightmap.sun_mask_size.x,
                    lightmap.sun_mask_size.y
                );
                RuntimeLightmapHandles {
                    primary: lightmap.primary_image.map(|image| images.add(image)),
                    secondary: lightmap.secondary_image.map(|image| images.add(image)),
                    secondary_b: lightmap.secondary_b_image.map(|image| images.add(image)),
                    ambient_diagnostic: images.add(lightmap.ambient_image),
                    directional_diagnostic: images.add(lightmap.directional_image),
                    sun_mask_diagnostic: images.add(lightmap.sun_mask_image),
                }
            });
            // One lightmap page is six adds and they cannot be split, so the
            // page is the indivisible unit here, not the image.
            self.note_step(step.elapsed(), 0);
            if self.lightmap_handles[self.lightmap_at].is_some() {
                self.done = self.done.saturating_add(1);
            }
            self.lightmap_at += 1;
            stepped = true;
            self.sync_stage();
        }
        self.upload_stage = None;
        true
    }

    fn sync_stage(&self) {
        if let Some(stage) = &self.upload_stage {
            stage.set_done(u64::from(self.done));
        }
    }
}

fn reachable_exact_image_slots(
    catalog: &crate::assemble::drawsurf::RuntimeMaterialCatalog,
    names: &[String],
) -> HashSet<u32> {
    let mut ids = HashSet::new();
    for (index, name) in names.iter().enumerate() {
        if name.starts_with('$') {
            ids.insert(index as u32);
        }
    }
    for material in &catalog.materials {
        for (_, texture) in &material.textures {
            if let Some(binding) = texture {
                ids.insert(binding.image.0);
            }
        }
    }
    if ids.is_empty() {
        ids.extend(0..names.len() as u32);
    }
    ids
}
