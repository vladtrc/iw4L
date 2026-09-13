use bevy::asset::AssetId;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::render_asset::{ExtractedAssets, RenderAssets};
use bevy::render::render_resource::{
    AddressMode, Buffer, FilterMode, MipmapFilterMode, SamplerDescriptor, TextureView,
    TextureViewDimension, TextureViewId,
};
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use d3d9_state::{
    AddressMode as D3dAddressMode, DecodedSamplerState, SamplerDecodeError, TextureFilter,
};
use render_frame::{SurfaceLightmapId, SurfaceReflectionProbeId, SurfaceSamplerInputs};
use render_material::{
    ExecutablePassView, MaterialGenerationId, RuntimeImageId, SamplerSource,
    SamplerTextureDimension,
};

use super::AdmittedExactPort;

pub use assets::image_handles::{
    RuntimeImageHandles, RuntimeImageHandlesData, RuntimeLightmapHandles,
};

pub const LENS_VIEW_SIGNATURE: (bevy::render::render_resource::TextureFormat, u32) =
    (bevy::render::render_resource::TextureFormat::Rgba8Unorm, 1);

#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct ColourWorkingSet {
    pub hits: u32,

    pub pipeline_not_ready: u32,
}

#[derive(Resource, Debug, Default, Clone, Copy)]
pub struct WorldPipelineWarmup {
    pub generation: frame::WorldGeneration,
    pub initialized: bool,
    pub total: u32,
    pub ready: u32,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct GpuSubmitReady {
    pub world_generation: frame::WorldGeneration,

    pub warm_pipelines: bool,

    pub overlay_gpu_wait: bool,
    pub pipeline_world_materials: std::sync::Arc<std::collections::HashSet<u16>>,
    pub pipeline_smodel_materials: std::sync::Arc<std::collections::HashSet<u16>>,
}

pub const UPLOAD_GRANULE: usize = 64 * 1024;

pub fn padded_upload_len(len: usize) -> usize {
    len.next_multiple_of(UPLOAD_GRANULE)
}

pub fn write_buffer_padded(queue: &RenderQueue, buffer: &Buffer, bytes: &[u8]) {
    let Some(size) = std::num::NonZeroU64::new(padded_upload_len(bytes.len()) as u64) else {
        return;
    };
    let mut view = queue
        .write_buffer_with(buffer, 0, size)
        .expect("padded upload fits its buffer");
    view.slice(..bytes.len()).copy_from_slice(bytes);
    view.slice(bytes.len()..).fill(0);
}

pub fn write_buffer_range(queue: &RenderQueue, buffer: &Buffer, offset: u64, bytes: &[u8]) {
    let Some(size) = std::num::NonZeroU64::new(bytes.len() as u64) else {
        return;
    };
    debug_assert_eq!(
        offset % 4,
        0,
        "constant arena range offset is 4-byte aligned"
    );
    debug_assert_eq!(
        bytes.len() % 4,
        0,
        "constant arena range length is 4-byte aligned"
    );
    let mut view = queue
        .write_buffer_with(buffer, offset, size)
        .expect("range upload fits its buffer");
    view.copy_from_slice(bytes);
}

pub fn require_image_generation(
    expected: MaterialGenerationId,
    table: MaterialGenerationId,
) -> Result<(), TextureBindRefusal> {
    if expected == table {
        Ok(())
    } else {
        Err(TextureBindRefusal::StaleImageGeneration {
            retained: table,
            current: expected,
        })
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub struct ExtractedRuntimeImageHandles {
    pub handles: RuntimeImageHandles,
}

impl std::ops::Deref for ExtractedRuntimeImageHandles {
    type Target = RuntimeImageHandles;

    fn deref(&self) -> &Self::Target {
        &self.handles
    }
}

#[derive(Clone, Debug)]
pub struct RuntimeUploadedLightmapViews {
    pub primary: Result<TextureView, UploadedViewRefusal>,
    pub secondary: Result<TextureView, UploadedViewRefusal>,
    pub secondary_b: Result<TextureView, UploadedViewRefusal>,
}

#[derive(Clone, Debug)]
pub struct UploadedTextureView {
    pub view: TextureView,
    pub dimension: TextureViewDimension,
}

#[derive(Resource, Clone, Debug, Default)]
pub struct RuntimeUploadedImageRegistry {
    source_handles: Option<RuntimeImageHandles>,
    slot_index: UploadedSlotIndex,
    pending: UploadedPending,
    pub generation_id: MaterialGenerationId,

    views_revision: u64,

    replaced_revision: u64,
    pub material_images: Vec<Option<Result<UploadedTextureView, UploadedViewRefusal>>>,
    pub reflection_probes: Vec<Option<Result<TextureView, UploadedViewRefusal>>>,
    pub lightmaps: Vec<Option<RuntimeUploadedLightmapViews>>,
    pub model_lighting: Option<Result<TextureView, UploadedViewRefusal>>,

    pub resolved_post_sun: Option<Result<TextureView, UploadedViewRefusal>>,
    pub float_z: Option<Result<TextureView, UploadedViewRefusal>>,

    pub sun_shadow: Option<Result<TextureView, UploadedViewRefusal>>,

    pub spot_shadow_rt10: Option<Result<TextureView, UploadedViewRefusal>>,

    pub spot_shadow_rt11: Option<Result<TextureView, UploadedViewRefusal>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub(crate) enum ViewChange {
    None,

    Published,

    Replaced,
}

impl ViewChange {
    fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::Replaced, _) | (_, Self::Replaced) => Self::Replaced,
            (Self::Published, _) | (_, Self::Published) => Self::Published,
            _ => Self::None,
        }
    }

    fn of_view(before: Option<TextureViewId>, after: Option<TextureViewId>) -> Self {
        match (before, after) {
            (a, b) if a == b => Self::None,
            (Some(_), Some(_)) => Self::Replaced,
            _ => Self::Published,
        }
    }
}

impl RuntimeUploadedImageRegistry {
    pub(crate) fn views_revision(&self) -> u64 {
        self.views_revision
    }

    pub(crate) fn replaced_revision(&self) -> u64 {
        self.replaced_revision
    }

    fn note(&mut self, change: ViewChange) {
        match change {
            ViewChange::None => {}
            ViewChange::Published => {
                self.views_revision = self.views_revision.wrapping_add(1);
            }
            ViewChange::Replaced => {
                self.views_revision = self.views_revision.wrapping_add(1);
                self.replaced_revision = self.replaced_revision.wrapping_add(1);
            }
        }
    }

    pub(crate) fn publish_frame_target(
        &mut self,
        select: impl FnOnce(&mut Self) -> &mut Option<Result<TextureView, UploadedViewRefusal>>,
        view: Option<TextureView>,
    ) {
        let change = {
            let slot = select(self);
            let before = match slot.as_ref() {
                Some(Ok(view)) => Some(view.id()),
                _ => None,
            };
            let after = view.as_ref().map(TextureView::id);
            *slot = view.map(Ok);
            ViewChange::of_view(before, after)
        };
        self.note(change);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UploadedViewRefusal {
    ExactImageNotRetained,
    StaleGeneration {
        retained: MaterialGenerationId,
        current: MaterialGenerationId,
    },
    UploadPending,
    DimensionMismatch {
        expected: TextureViewDimension,
        actual: TextureViewDimension,
    },
}

#[derive(Clone, Debug, Default)]
struct UploadedSlotIndex {
    material: HashMap<AssetId<Image>, Vec<u32>>,
    probes: HashMap<AssetId<Image>, Vec<u32>>,
    lightmaps: HashMap<AssetId<Image>, Vec<(u32, u8)>>,
    model_lighting: Option<AssetId<Image>>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct UploadedPending {
    material: Vec<u32>,
    probes: Vec<u32>,
    lightmaps: Vec<(u32, u8)>,
    model_lighting: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum UploadedRefresh {
    None,
    Slots(UploadedRefreshSlots),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct UploadedRefreshSlots {
    material: Vec<u32>,
    probes: Vec<u32>,
    lightmaps: Vec<(u32, u8)>,
    model_lighting: bool,
}

pub(crate) fn prepare_uploaded_image_registry(
    handles: Option<Res<ExtractedRuntimeImageHandles>>,
    gpu_images: Res<RenderAssets<GpuImage>>,
    extracted: Option<Res<ExtractedAssets<GpuImage>>>,
    mut registry: ResMut<RuntimeUploadedImageRegistry>,
) {
    let Some(handles) = handles else {
        if registry.source_handles.is_none() {
            return;
        }
        let (views, replaced) = (registry.views_revision, registry.replaced_revision);
        *registry = RuntimeUploadedImageRegistry::default();
        registry.views_revision = views.wrapping_add(1);
        registry.replaced_revision = replaced.wrapping_add(1);
        return;
    };
    let source_unchanged = registry
        .source_handles
        .as_ref()
        .is_some_and(|retained| retained.ptr_eq(&handles.handles));
    if !source_unchanged {
        registry.source_handles = Some(handles.handles.clone());
        registry.slot_index = UploadedSlotIndex::from_handles(&handles.handles);
        registry.generation_id = handles.generation_id;
        let change = refresh_all_uploaded_slots(&mut registry, &handles, &gpu_images);
        registry.note(change);
        registry.pending = uploaded_pending_from_registry(&registry);
        return;
    }

    let plan = match extracted.as_ref() {
        Some(extracted) => uploaded_refresh_plan(
            extracted
                .added
                .iter()
                .copied()
                .chain(extracted.modified.iter().copied()),
            &registry.pending,
            &registry.slot_index,
        ),
        None => uploaded_refresh_plan(core::iter::empty(), &registry.pending, &registry.slot_index),
    };
    match plan {
        UploadedRefresh::None => {}
        UploadedRefresh::Slots(slots) => {
            let change = refresh_uploaded_slots(&mut registry, &handles, &gpu_images, &slots);
            registry.note(change);
            retarget_uploaded_pending(&mut registry, &slots);
        }
    }
}

impl UploadedSlotIndex {
    fn from_handles(handles: &RuntimeImageHandlesData) -> Self {
        let mut index = Self::default();
        for (slot, handle) in handles.material_images.iter().enumerate() {
            if let Some(handle) = handle {
                index
                    .material
                    .entry(handle.id())
                    .or_default()
                    .push(slot as u32);
            }
        }
        for (slot, handle) in handles.reflection_probes.iter().enumerate() {
            if let Some(handle) = handle {
                index
                    .probes
                    .entry(handle.id())
                    .or_default()
                    .push(slot as u32);
            }
        }
        for (slot, page) in handles.lightmaps.iter().enumerate() {
            let Some(page) = page else {
                continue;
            };
            let slot = slot as u32;
            for (plane, handle) in [
                (0u8, page.primary.as_ref()),
                (1, page.secondary.as_ref()),
                (2, page.secondary_b.as_ref()),
            ] {
                if let Some(handle) = handle {
                    index
                        .lightmaps
                        .entry(handle.id())
                        .or_default()
                        .push((slot, plane));
                }
            }
        }
        index.model_lighting = handles.model_lighting.as_ref().map(Handle::id);
        index
    }
}

fn uploaded_refresh_plan(
    changed: impl IntoIterator<Item = AssetId<Image>>,
    pending: &UploadedPending,
    index: &UploadedSlotIndex,
) -> UploadedRefresh {
    let mut slots = UploadedRefreshSlots {
        material: pending.material.clone(),
        probes: pending.probes.clone(),
        lightmaps: pending.lightmaps.clone(),
        model_lighting: pending.model_lighting,
    };
    for id in changed {
        if let Some(owned) = index.material.get(&id) {
            for &slot in owned {
                push_unique_u32(&mut slots.material, slot);
            }
        }
        if let Some(owned) = index.probes.get(&id) {
            for &slot in owned {
                push_unique_u32(&mut slots.probes, slot);
            }
        }
        if let Some(owned) = index.lightmaps.get(&id) {
            for &key in owned {
                push_unique_lightmap(&mut slots.lightmaps, key);
            }
        }
        if index.model_lighting == Some(id) {
            slots.model_lighting = true;
        }
    }
    if slots.material.is_empty()
        && slots.probes.is_empty()
        && slots.lightmaps.is_empty()
        && !slots.model_lighting
    {
        UploadedRefresh::None
    } else {
        UploadedRefresh::Slots(slots)
    }
}

fn push_unique_u32(out: &mut Vec<u32>, slot: u32) {
    if !out.contains(&slot) {
        out.push(slot);
    }
}

fn push_unique_lightmap(out: &mut Vec<(u32, u8)>, key: (u32, u8)) {
    if !out.contains(&key) {
        out.push(key);
    }
}

fn refresh_all_uploaded_slots(
    registry: &mut RuntimeUploadedImageRegistry,
    handles: &RuntimeImageHandlesData,
    gpu_images: &RenderAssets<GpuImage>,
) -> ViewChange {
    let mut change = if registry.material_images.len() != handles.material_images.len()
        || registry.reflection_probes.len() != handles.reflection_probes.len()
        || registry.lightmaps.len() != handles.lightmaps.len()
    {
        ViewChange::Published
    } else {
        ViewChange::None
    };
    registry
        .material_images
        .resize_with(handles.material_images.len(), || None);
    for (slot, handle) in registry
        .material_images
        .iter_mut()
        .zip(&handles.material_images)
    {
        change = change.merge(refresh_native_view(slot, handle.as_ref(), gpu_images));
    }
    registry
        .reflection_probes
        .resize_with(handles.reflection_probes.len(), || None);
    for (index, (slot, handle)) in registry
        .reflection_probes
        .iter_mut()
        .zip(&handles.reflection_probes)
        .enumerate()
    {
        if let Some(handle) = handle {
            log_uploaded_probe_once(gpu_images, handle, index);
        }
        change = change.merge(refresh_view(
            slot,
            handle.as_ref(),
            gpu_images,
            TextureViewDimension::Cube,
        ));
    }
    registry
        .lightmaps
        .resize_with(handles.lightmaps.len(), || None);
    for (slot, page) in registry.lightmaps.iter_mut().zip(&handles.lightmaps) {
        change = change.merge(refresh_lightmap_page(slot, page.as_ref(), gpu_images));
    }
    change.merge(refresh_view(
        &mut registry.model_lighting,
        handles.model_lighting.as_ref(),
        gpu_images,
        TextureViewDimension::D3,
    ))
}

fn refresh_uploaded_slots(
    registry: &mut RuntimeUploadedImageRegistry,
    handles: &RuntimeImageHandlesData,
    gpu_images: &RenderAssets<GpuImage>,
    slots: &UploadedRefreshSlots,
) -> ViewChange {
    let mut change = ViewChange::None;
    for &slot in &slots.material {
        let Some(dst) = registry.material_images.get_mut(slot as usize) else {
            continue;
        };
        change = change.merge(refresh_native_view(
            dst,
            handles
                .material_images
                .get(slot as usize)
                .and_then(Option::as_ref),
            gpu_images,
        ));
    }
    for &slot in &slots.probes {
        let Some(dst) = registry.reflection_probes.get_mut(slot as usize) else {
            continue;
        };
        let handle = handles
            .reflection_probes
            .get(slot as usize)
            .and_then(Option::as_ref);
        if let Some(handle) = handle {
            log_uploaded_probe_once(gpu_images, handle, slot as usize);
        }
        change = change.merge(refresh_view(
            dst,
            handle,
            gpu_images,
            TextureViewDimension::Cube,
        ));
    }
    for &(slot, plane) in &slots.lightmaps {
        let Some(page) = handles.lightmaps.get(slot as usize) else {
            continue;
        };
        let Some(page) = page else {
            if let Some(dst) = registry.lightmaps.get_mut(slot as usize) {
                change = change.merge(refresh_lightmap_page(dst, None, gpu_images));
            }
            continue;
        };
        if registry
            .lightmaps
            .get(slot as usize)
            .is_some_and(Option::is_none)
        {
            change = change.merge(ViewChange::Published);
        }
        let Some(views) = registry.lightmaps.get_mut(slot as usize).map(|slot| {
            slot.get_or_insert_with(|| RuntimeUploadedLightmapViews {
                primary: Err(UploadedViewRefusal::ExactImageNotRetained),
                secondary: Err(UploadedViewRefusal::ExactImageNotRetained),
                secondary_b: Err(UploadedViewRefusal::ExactImageNotRetained),
            })
        }) else {
            continue;
        };
        change = change.merge(match plane {
            0 => refresh_required_view(
                &mut views.primary,
                page.primary.as_ref(),
                gpu_images,
                TextureViewDimension::D2,
            ),
            1 => refresh_required_view(
                &mut views.secondary,
                page.secondary.as_ref(),
                gpu_images,
                TextureViewDimension::D2,
            ),
            _ => refresh_required_view(
                &mut views.secondary_b,
                page.secondary_b.as_ref(),
                gpu_images,
                TextureViewDimension::D2,
            ),
        });
    }
    if slots.model_lighting {
        change = change.merge(refresh_view(
            &mut registry.model_lighting,
            handles.model_lighting.as_ref(),
            gpu_images,
            TextureViewDimension::D3,
        ));
    }
    change
}

fn refresh_lightmap_page(
    slot: &mut Option<RuntimeUploadedLightmapViews>,
    page: Option<&RuntimeLightmapHandles>,
    gpu_images: &RenderAssets<GpuImage>,
) -> ViewChange {
    let Some(page) = page else {
        return match slot.take() {
            Some(_) => ViewChange::Published,
            None => ViewChange::None,
        };
    };
    let mut change = if slot.is_none() {
        ViewChange::Published
    } else {
        ViewChange::None
    };
    let views = slot.get_or_insert_with(|| RuntimeUploadedLightmapViews {
        primary: Err(UploadedViewRefusal::ExactImageNotRetained),
        secondary: Err(UploadedViewRefusal::ExactImageNotRetained),
        secondary_b: Err(UploadedViewRefusal::ExactImageNotRetained),
    });
    change = change.merge(refresh_required_view(
        &mut views.primary,
        page.primary.as_ref(),
        gpu_images,
        TextureViewDimension::D2,
    ));
    change = change.merge(refresh_required_view(
        &mut views.secondary,
        page.secondary.as_ref(),
        gpu_images,
        TextureViewDimension::D2,
    ));
    change.merge(refresh_required_view(
        &mut views.secondary_b,
        page.secondary_b.as_ref(),
        gpu_images,
        TextureViewDimension::D2,
    ))
}

fn uploaded_pending_from_registry(registry: &RuntimeUploadedImageRegistry) -> UploadedPending {
    let mut pending = UploadedPending::default();
    for (slot, view) in registry.material_images.iter().enumerate() {
        if material_slot_pending(view) {
            pending.material.push(slot as u32);
        }
    }
    for (slot, view) in registry.reflection_probes.iter().enumerate() {
        if probe_slot_pending(view) {
            pending.probes.push(slot as u32);
        }
    }
    for (slot, page) in registry.lightmaps.iter().enumerate() {
        let Some(page) = page else {
            continue;
        };
        if matches!(page.primary, Err(UploadedViewRefusal::UploadPending)) {
            pending.lightmaps.push((slot as u32, 0));
        }
        if matches!(page.secondary, Err(UploadedViewRefusal::UploadPending)) {
            pending.lightmaps.push((slot as u32, 1));
        }
        if matches!(page.secondary_b, Err(UploadedViewRefusal::UploadPending)) {
            pending.lightmaps.push((slot as u32, 2));
        }
    }
    pending.model_lighting = matches!(
        registry.model_lighting,
        Some(Err(UploadedViewRefusal::UploadPending))
    );
    pending
}

fn retarget_uploaded_pending(
    registry: &mut RuntimeUploadedImageRegistry,
    slots: &UploadedRefreshSlots,
) {
    registry.pending.material.retain(|&slot| {
        material_slot_pending(registry.material_images.get(slot as usize).unwrap_or(&None))
    });
    registry.pending.probes.retain(|&slot| {
        probe_slot_pending(
            registry
                .reflection_probes
                .get(slot as usize)
                .unwrap_or(&None),
        )
    });
    registry.pending.lightmaps.retain(|&(slot, plane)| {
        lightmap_plane_pending(
            registry
                .lightmaps
                .get(slot as usize)
                .and_then(|page| page.as_ref()),
            plane,
        )
    });
    registry.pending.model_lighting = matches!(
        registry.model_lighting,
        Some(Err(UploadedViewRefusal::UploadPending))
    );
    for &slot in &slots.material {
        if material_slot_pending(registry.material_images.get(slot as usize).unwrap_or(&None)) {
            push_unique_u32(&mut registry.pending.material, slot);
        }
    }
    for &slot in &slots.probes {
        if probe_slot_pending(
            registry
                .reflection_probes
                .get(slot as usize)
                .unwrap_or(&None),
        ) {
            push_unique_u32(&mut registry.pending.probes, slot);
        }
    }
    for &key in &slots.lightmaps {
        if lightmap_plane_pending(
            registry
                .lightmaps
                .get(key.0 as usize)
                .and_then(|page| page.as_ref()),
            key.1,
        ) {
            push_unique_lightmap(&mut registry.pending.lightmaps, key);
        }
    }
    if slots.model_lighting {
        registry.pending.model_lighting = matches!(
            registry.model_lighting,
            Some(Err(UploadedViewRefusal::UploadPending))
        );
    }
}

fn material_slot_pending(slot: &Option<Result<UploadedTextureView, UploadedViewRefusal>>) -> bool {
    matches!(slot, Some(Err(UploadedViewRefusal::UploadPending)))
}

fn probe_slot_pending(slot: &Option<Result<TextureView, UploadedViewRefusal>>) -> bool {
    matches!(slot, Some(Err(UploadedViewRefusal::UploadPending)))
}

fn lightmap_plane_pending(page: Option<&RuntimeUploadedLightmapViews>, plane: u8) -> bool {
    let Some(page) = page else {
        return false;
    };
    let slot = match plane {
        0 => &page.primary,
        1 => &page.secondary,
        _ => &page.secondary_b,
    };
    matches!(slot, Err(UploadedViewRefusal::UploadPending))
}

fn refresh_native_view(
    slot: &mut Option<Result<UploadedTextureView, UploadedViewRefusal>>,
    handle: Option<&Handle<Image>>,
    gpu_images: &RenderAssets<GpuImage>,
) -> ViewChange {
    let bound = |slot: &Option<Result<UploadedTextureView, UploadedViewRefusal>>| match slot {
        Some(Ok(current)) => Some((current.view.id(), current.dimension)),
        _ => None,
    };
    let before = bound(slot);
    let Some(handle) = handle else {
        *slot = None;
        return ViewChange::of_view(before.map(|(id, _)| id), None);
    };
    let Some(gpu) = gpu_images.get(handle) else {
        if before.is_none() && matches!(slot, Some(Err(UploadedViewRefusal::UploadPending))) {
            return ViewChange::None;
        }
        *slot = Some(Err(UploadedViewRefusal::UploadPending));
        return ViewChange::of_view(before.map(|(id, _)| id), None);
    };
    let dimension = gpu
        .texture_view_descriptor
        .as_ref()
        .and_then(|descriptor| descriptor.dimension)
        .unwrap_or(TextureViewDimension::D2);
    if before == Some((gpu.texture_view.id(), dimension)) {
        return ViewChange::None;
    }
    *slot = Some(Ok(UploadedTextureView {
        view: gpu.texture_view.clone(),
        dimension,
    }));
    ViewChange::of_view(before.map(|(id, _)| id), Some(gpu.texture_view.id()))
}

fn refresh_view(
    slot: &mut Option<Result<TextureView, UploadedViewRefusal>>,
    handle: Option<&Handle<Image>>,
    gpu_images: &RenderAssets<GpuImage>,
    expected: TextureViewDimension,
) -> ViewChange {
    let Some(handle) = handle else {
        let before = match slot.as_ref() {
            Some(Ok(view)) => Some(view.id()),
            _ => None,
        };
        let existed = slot.take().is_some();
        return match (before, existed) {
            (Some(_), _) => ViewChange::Replaced,
            (None, true) => ViewChange::Published,
            (None, false) => ViewChange::None,
        };
    };
    let fresh = slot.is_none();
    let change = refresh_required_view(
        slot.get_or_insert(Err(UploadedViewRefusal::UploadPending)),
        Some(handle),
        gpu_images,
        expected,
    );
    if fresh {
        change.merge(ViewChange::Published)
    } else {
        change
    }
}

fn refresh_required_view(
    slot: &mut Result<TextureView, UploadedViewRefusal>,
    handle: Option<&Handle<Image>>,
    gpu_images: &RenderAssets<GpuImage>,
    expected: TextureViewDimension,
) -> ViewChange {
    let refuse = |slot: &mut Result<TextureView, UploadedViewRefusal>, next| match slot.as_ref() {
        Err(current) if *current == next => ViewChange::None,
        _ => {
            *slot = Err(next);
            ViewChange::Published
        }
    };
    let Some(handle) = handle else {
        return refuse(slot, UploadedViewRefusal::ExactImageNotRetained);
    };
    let Some(gpu) = gpu_images.get(handle) else {
        return refuse(slot, UploadedViewRefusal::UploadPending);
    };
    let actual = gpu
        .texture_view_descriptor
        .as_ref()
        .and_then(|descriptor| descriptor.dimension)
        .unwrap_or(TextureViewDimension::D2);
    if actual != expected {
        return refuse(
            slot,
            UploadedViewRefusal::DimensionMismatch { expected, actual },
        );
    }
    let before = slot.as_ref().ok().map(TextureView::id);
    if before == Some(gpu.texture_view.id()) {
        return ViewChange::None;
    }
    *slot = Ok(gpu.texture_view.clone());
    ViewChange::of_view(before, Some(gpu.texture_view.id()))
}

fn log_uploaded_probe_once(
    gpu_images: &RenderAssets<GpuImage>,
    handle: &Handle<Image>,
    index: usize,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static LOGGED: AtomicUsize = AtomicUsize::new(0);
    if index >= 4 || LOGGED.fetch_max(index + 1, Ordering::Relaxed) > index {
        return;
    }
    let Some(gpu) = gpu_images.get(handle) else {
        return;
    };
    diag::warn!(
        World,
        "probe cube[{index}] uploaded {}x{} layers={} mips={} format={:?} had_data={} view_dim={:?} view_mips={:?}",
        gpu.texture_descriptor.size.width,
        gpu.texture_descriptor.size.height,
        gpu.texture_descriptor.size.depth_or_array_layers,
        gpu.texture_descriptor.mip_level_count,
        gpu.texture_descriptor.format,
        gpu.had_data,
        gpu.texture_view_descriptor
            .as_ref()
            .and_then(|descriptor| descriptor.dimension),
        gpu.texture_view_descriptor
            .as_ref()
            .and_then(|descriptor| descriptor.mip_level_count),
    );
}

#[derive(Resource, Clone, Copy, Debug, PartialEq, Eq)]
pub struct RetailSamplerRendererInputs {
    pub max_anisotropy: u32,

    pub min_anisotropy: u32,

    pub device_max_anisotropy: u32,

    pub supports_min_anisotropic: bool,

    pub supports_mag_anisotropic: bool,

    pub disable_filtering: bool,

    pub mip_mode: u8,
}

pub const RETAIL_SAMPLER_PROFILE_2026_08_11: RetailSamplerRendererInputs =
    RetailSamplerRendererInputs {
        max_anisotropy: 4,
        min_anisotropy: 1,
        device_max_anisotropy: 16,
        supports_min_anisotropic: true,
        supports_mag_anisotropic: true,
        disable_filtering: false,
        mip_mode: 0,
    };

pub const RETAIL_SAMPLER_WORDS_2026_08_11: [u32; 24] = [
    0x001101, 0x001101, 0x002201, 0x003302, 0x003304, 0x001101, 0x001101, 0x001101, 0x011101,
    0x011101, 0x012201, 0x013302, 0x013304, 0x011101, 0x011101, 0x011101, 0x021101, 0x021101,
    0x022201, 0x023302, 0x023304, 0x021101, 0x021101, 0x021101,
];

#[derive(Resource, Clone, Debug, PartialEq, Eq)]
pub struct RetailSamplerTable {
    words: [u32; 24],
}

const RETAIL_MIP_FILTERS: [u32; 12] = [0, 1, 2, 0, 2, 2, 0, 1, 1, 0, 0, 0];

impl RetailSamplerTable {
    pub fn produce(inputs: RetailSamplerRendererInputs) -> Result<Self, SamplerTableBuildRefusal> {
        let mip_mode = usize::from(inputs.mip_mode);
        if mip_mode >= 4 {
            return Err(SamplerTableBuildRefusal::MipModeOutOfRange {
                mip_mode: inputs.mip_mode,
            });
        }

        let mut max_anisotropy = inputs.max_anisotropy.min(inputs.device_max_anisotropy);
        let mut anisotropic_filters = 0x2200;
        if max_anisotropy < 2 {
            max_anisotropy = 1;
        } else {
            let min_filter = if inputs.supports_min_anisotropic {
                3
            } else {
                2
            };
            let mag_filter = if inputs.supports_mag_anisotropic {
                3
            } else {
                2
            };
            anisotropic_filters = (min_filter << 8) | (mag_filter << 12);
        }

        let mut min_anisotropy = inputs.min_anisotropy.min(max_anisotropy);
        let mut minimum_filters = if min_anisotropy == 1 {
            0x2200
        } else {
            anisotropic_filters
        };
        let mut two_x = if min_anisotropy <= max_anisotropy.min(2) {
            max_anisotropy.min(2)
        } else {
            min_anisotropy
        };
        let mut four_x = if min_anisotropy <= max_anisotropy.min(4) {
            max_anisotropy.min(4)
        } else {
            min_anisotropy
        };
        let mut ordinary_filters = 0x2200;
        let mut effective_mip_mode = mip_mode;
        if inputs.disable_filtering {
            min_anisotropy = 1;
            two_x = 1;
            four_x = 1;
            anisotropic_filters = 0x1100;
            minimum_filters = 0x1100;
            ordinary_filters = 0x1100;
            effective_mip_mode = 3;
        }

        let mut words = [0u32; 24];
        for (index, word) in words.iter_mut().enumerate() {
            let low_filter_bits = match index & 7 {
                2 => {
                    let mip = RETAIL_MIP_FILTERS[effective_mip_mode * 3 + index / 8] << 16;
                    if mip == 0 {
                        ordinary_filters | 1
                    } else {
                        mip | minimum_filters | min_anisotropy
                    }
                }
                3 => anisotropic_filters | two_x,
                4 => anisotropic_filters | four_x,
                _ => 0x1101,
            };
            let mip_filter = RETAIL_MIP_FILTERS[effective_mip_mode * 3 + index / 8] << 16;
            *word = if index & 7 == 2 {
                low_filter_bits
            } else {
                mip_filter | low_filter_bits
            };
        }
        Ok(Self { words })
    }

    pub fn captured_2026_08_11() -> Result<Self, SamplerTableBuildRefusal> {
        let table = Self::produce(RETAIL_SAMPLER_PROFILE_2026_08_11)?;
        if table.words != RETAIL_SAMPLER_WORDS_2026_08_11 {
            return Err(SamplerTableBuildRefusal::CapturedWordsMismatch {
                expected: RETAIL_SAMPLER_WORDS_2026_08_11,
                produced: table.words,
            });
        }
        Ok(table)
    }

    pub fn host_adapted_rows(&self) -> Vec<u8> {
        (0..24u8)
            .filter(|row| {
                self.decode(*row)
                    .is_ok_and(|sampler| sampler.needs_linear_mip_for_anisotropy())
            })
            .collect()
    }

    pub fn decode(&self, sampler_state: u8) -> Result<DecodedSampler, TextureBindRefusal> {
        let table_index = sampler_state & 0x1f;
        let word = *self.words.get(usize::from(table_index)).ok_or(
            TextureBindRefusal::SamplerTableIndexUninitialized {
                sampler_state,
                table_index,
            },
        )?;

        let state = DecodedSamplerState::from_packed_word_and_clamp_flags(
            word,
            sampler_state & 0x20 != 0,
            sampler_state & 0x40 != 0,
            sampler_state & 0x80 != 0,
        )
        .map_err(|err| map_sampler_decode_error(sampler_state, err))?;
        Ok(DecodedSampler {
            sampler_state,
            state,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DecodedSampler {
    pub sampler_state: u8,
    pub state: DecodedSamplerState,
}

impl DecodedSampler {
    pub fn packed_word(self) -> u32 {
        self.state.packed_word
    }

    pub fn uses_mipmaps(self) -> bool {
        self.state.uses_mipmaps
    }

    pub fn anisotropy_clamp(self) -> u16 {
        self.state.anisotropy_clamp
    }

    pub fn needs_linear_mip_for_anisotropy(&self) -> bool {
        self.state.anisotropy_clamp > 1
            && d3d_mip_to_wgpu(self.state.mip_filter) != MipmapFilterMode::Linear
    }

    pub fn descriptor(&self) -> SamplerDescriptor<'static> {
        SamplerDescriptor {
            label: Some("iw4 exact material sampler"),
            address_mode_u: d3d_address_to_wgpu(self.state.address_u),
            address_mode_v: d3d_address_to_wgpu(self.state.address_v),
            address_mode_w: d3d_address_to_wgpu(self.state.address_w),
            mag_filter: d3d_min_mag_to_wgpu(self.state.mag_filter),
            min_filter: d3d_min_mag_to_wgpu(self.state.min_filter),
            mipmap_filter: if self.needs_linear_mip_for_anisotropy() {
                MipmapFilterMode::Linear
            } else {
                d3d_mip_to_wgpu(self.state.mip_filter)
            },
            lod_max_clamp: if self.state.uses_mipmaps { 32.0 } else { 0.0 },
            anisotropy_clamp: self.state.anisotropy_clamp,
            ..Default::default()
        }
    }
}

fn map_sampler_decode_error(sampler_state: u8, err: SamplerDecodeError) -> TextureBindRefusal {
    match err {
        SamplerDecodeError::UnsupportedMinMagFilter { packed_word, raw } => {
            TextureBindRefusal::UnsupportedMinMagFilter {
                sampler_state,
                packed_word,
                raw,
            }
        }
        SamplerDecodeError::UnsupportedMipFilter { packed_word, raw } => {
            TextureBindRefusal::UnsupportedMipFilter {
                sampler_state,
                packed_word,
                raw,
            }
        }
        SamplerDecodeError::AnisotropyOverflow {
            packed_word,
            anisotropy,
        } => TextureBindRefusal::AnisotropyOverflow {
            sampler_state,
            packed_word,
            anisotropy,
        },
    }
}

fn d3d_address_to_wgpu(mode: D3dAddressMode) -> AddressMode {
    match mode {
        D3dAddressMode::Wrap => AddressMode::Repeat,
        D3dAddressMode::Mirror => AddressMode::MirrorRepeat,
        D3dAddressMode::Clamp => AddressMode::ClampToEdge,
        D3dAddressMode::Border => AddressMode::ClampToBorder,

        D3dAddressMode::MirrorOnce | D3dAddressMode::Unknown(_) => AddressMode::ClampToEdge,
    }
}

fn d3d_min_mag_to_wgpu(filter: TextureFilter) -> FilterMode {
    match filter {
        TextureFilter::Point => FilterMode::Nearest,
        TextureFilter::Linear | TextureFilter::Anisotropic => FilterMode::Linear,

        TextureFilter::None | TextureFilter::Unknown(_) => FilterMode::Nearest,
    }
}

fn d3d_mip_to_wgpu(filter: TextureFilter) -> MipmapFilterMode {
    match filter {
        TextureFilter::None | TextureFilter::Point => MipmapFilterMode::Nearest,
        TextureFilter::Linear | TextureFilter::Anisotropic => MipmapFilterMode::Linear,
        TextureFilter::Unknown(_) => MipmapFilterMode::Nearest,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UploadedTextureIdentity {
    Material(RuntimeImageId),
    Code(u32),
    SpotShadowRt(u8),
    ReflectionProbe(SurfaceReflectionProbeId),
    PrimaryLightmap(SurfaceLightmapId),
    SecondaryLightmap(SurfaceLightmapId),
    SecondaryBLightmap(SurfaceLightmapId),
}

#[derive(Clone, Debug)]
pub struct UploadedTextureBind {
    pub register: u16,
    pub dimension: SamplerTextureDimension,
    pub identity: UploadedTextureIdentity,
    pub view: TextureView,
    pub sampler: DecodedSampler,
}

pub(crate) trait RuntimeProgramPortGpuExt {
    fn resolve_uploaded_texture_binds(
        &self,
        pass: ExecutablePassView<'_>,
        surface: SurfaceSamplerInputs,
        images: &RuntimeImageHandles,
        expected: MaterialGenerationId,
        uploaded: &RuntimeUploadedImageRegistry,
        samplers: &RetailSamplerTable,
        spot_shadow_select: Option<u8>,
    ) -> Result<Vec<UploadedTextureBind>, TextureBindRefusal>;
}

impl RuntimeProgramPortGpuExt for AdmittedExactPort {
    fn resolve_uploaded_texture_binds(
        &self,
        pass: ExecutablePassView<'_>,
        surface: SurfaceSamplerInputs,
        images: &RuntimeImageHandles,
        expected: MaterialGenerationId,
        uploaded: &RuntimeUploadedImageRegistry,
        samplers: &RetailSamplerTable,
        spot_shadow_select: Option<u8>,
    ) -> Result<Vec<UploadedTextureBind>, TextureBindRefusal> {
        require_image_generation(expected, images.generation_id)?;
        require_image_generation(expected, uploaded.generation_id)?;
        if !self.accepts_pass(pass) {
            return Err(TextureBindRefusal::ExecutablePairMismatch);
        }
        self.abi()
            .samplers
            .iter()
            .map(|binding| {
                let (view, sampler_state, identity) = match binding.source {
                    SamplerSource::MaterialTexture { name_hash } => {
                        let texture = packed_material_texture(pass, binding.register, name_hash)?;

                        let _ = material_image(images, texture.image, binding.register)?;
                        (
                            take_uploaded_material(
                                uploaded,
                                texture.image,
                                binding.register,
                                binding.dimension,
                            )?,
                            texture.sampler_state,
                            UploadedTextureIdentity::Material(texture.image),
                        )
                    }
                    SamplerSource::CodeTexture { index } => {
                        let lane = packed_code_texture(pass, binding.register, index)?;
                        if !is_runtime_render_target_code_image(index) {
                            let _ = code_image(images, index, binding.register, lane.image)?;
                        }
                        let (view, identity) = take_uploaded_code_image(
                            uploaded,
                            index,
                            binding.register,
                            lane.image,
                            spot_shadow_select,
                        )?;
                        (view, lane.sampler_state, identity)
                    }
                    SamplerSource::SurfaceReflectionProbe => {
                        let id = surface.reflection_probe.ok_or(
                            TextureBindRefusal::SurfaceReflectionProbeMissing {
                                register: binding.register,
                            },
                        )?;
                        (
                            take_uploaded_probe(uploaded, id, binding.register)?,
                            0x72,
                            UploadedTextureIdentity::ReflectionProbe(id),
                        )
                    }
                    SamplerSource::SurfacePrimaryLightmap => {
                        let id = surface.primary_lightmap.ok_or(
                            TextureBindRefusal::SurfacePrimaryLightmapMissing {
                                register: binding.register,
                            },
                        )?;
                        (
                            take_uploaded_lightmap(uploaded, id, true, binding.register)?,
                            0x62,
                            UploadedTextureIdentity::PrimaryLightmap(id),
                        )
                    }
                    SamplerSource::SurfaceSecondaryBLightmap => {
                        let id = surface.secondary_lightmap.ok_or(
                            TextureBindRefusal::SurfaceSecondaryLightmapMissing {
                                register: binding.register,
                            },
                        )?;
                        let page = uploaded
                            .lightmaps
                            .get(usize::from(id.0))
                            .and_then(Option::as_ref)
                            .ok_or(TextureBindRefusal::LightmapUnavailable {
                                register: binding.register,
                                id,
                            })?;
                        let view = page.secondary_b.clone().map_err(|cause| {
                            TextureBindRefusal::UploadedView {
                                register: binding.register,
                                cause,
                            }
                        })?;
                        (view, 0x62, UploadedTextureIdentity::SecondaryBLightmap(id))
                    }
                    SamplerSource::SurfaceSecondaryLightmap => {
                        let id = surface.secondary_lightmap.ok_or(
                            TextureBindRefusal::SurfaceSecondaryLightmapMissing {
                                register: binding.register,
                            },
                        )?;
                        (
                            take_uploaded_lightmap(uploaded, id, false, binding.register)?,
                            0x62,
                            UploadedTextureIdentity::SecondaryLightmap(id),
                        )
                    }
                };
                Ok(UploadedTextureBind {
                    register: binding.register,
                    dimension: binding.dimension,
                    identity,
                    view,
                    sampler: samplers.decode(sampler_state)?,
                })
            })
            .collect()
    }
}

fn packed_material_texture(
    pass: ExecutablePassView<'_>,
    register: u16,
    name_hash: u32,
) -> Result<super::RuntimeTextureBinding, TextureBindRefusal> {
    pass.local_samplers
        .and_then(|packed| packed.texture(register, name_hash))
        .ok_or(TextureBindRefusal::ExecutableSamplerArgumentMissing { register })
}

fn packed_code_texture(
    pass: ExecutablePassView<'_>,
    register: u16,
    index: u32,
) -> Result<super::PackedCodeSamplerLane, TextureBindRefusal> {
    pass.code_samplers
        .iter()
        .find(|lane| lane.register == register && lane.index == index)
        .copied()
        .ok_or(TextureBindRefusal::ExecutableSamplerArgumentMissing { register })
}

fn take_uploaded_material(
    uploaded: &RuntimeUploadedImageRegistry,
    id: RuntimeImageId,
    register: u16,
    expected: SamplerTextureDimension,
) -> Result<TextureView, TextureBindRefusal> {
    let uploaded = uploaded
        .material_images
        .get(id.0 as usize)
        .and_then(Option::as_ref)
        .ok_or(TextureBindRefusal::MaterialImageUnavailable { register, id })?
        .clone()
        .map_err(|cause| TextureBindRefusal::UploadedView { register, cause })?;
    let expected = match expected {
        SamplerTextureDimension::D2 => TextureViewDimension::D2,
        SamplerTextureDimension::Cube => TextureViewDimension::Cube,
        SamplerTextureDimension::D3 => TextureViewDimension::D3,
    };
    if uploaded.dimension != expected {
        return Err(TextureBindRefusal::UploadedViewDimensionMismatch {
            register,
            expected,
            actual: uploaded.dimension,
        });
    }
    Ok(uploaded.view)
}

fn is_runtime_render_target_code_image(index: u32) -> bool {
    index == crate::drawsurf::CODE_TEXTURE_RESOLVED_POST_SUN
        || index == crate::drawsurf::CODE_TEXTURE_FLOATZ
        || index == crate::drawsurf::CODE_TEXTURE_SHADOWMAP_SUN
        || index == crate::drawsurf::CODE_TEXTURE_SHADOWMAP_SPOT
}

fn take_uploaded_code_image(
    uploaded: &RuntimeUploadedImageRegistry,
    index: u32,
    register: u16,
    catalog_image: Option<RuntimeImageId>,
    spot_shadow_select: Option<u8>,
) -> Result<(TextureView, UploadedTextureIdentity), TextureBindRefusal> {
    if index == crate::drawsurf::CODE_TEXTURE_RESOLVED_POST_SUN {
        let view = uploaded
            .resolved_post_sun
            .clone()
            .ok_or(TextureBindRefusal::ResolvedPostSunUnavailable { register })?
            .map_err(|cause| TextureBindRefusal::UploadedView { register, cause })?;
        return Ok((view, UploadedTextureIdentity::Code(index)));
    }
    if index == crate::drawsurf::CODE_TEXTURE_FLOATZ {
        let view = uploaded
            .float_z
            .clone()
            .ok_or(TextureBindRefusal::FloatZUnresolved { register })?
            .map_err(|cause| TextureBindRefusal::UploadedView { register, cause })?;
        return Ok((view, UploadedTextureIdentity::Code(index)));
    }
    if index == crate::drawsurf::CODE_TEXTURE_SHADOWMAP_SUN {
        let view = uploaded
            .sun_shadow
            .clone()
            .ok_or(TextureBindRefusal::SunShadowUnresolved { register })?
            .map_err(|cause| TextureBindRefusal::UploadedView { register, cause })?;
        return Ok((view, UploadedTextureIdentity::Code(index)));
    }
    if index == crate::drawsurf::CODE_TEXTURE_SHADOWMAP_SPOT {
        let rt = spot_shadow_select.ok_or(TextureBindRefusal::SpotShadowUnresolved { register })?;
        let view = match rt {
            lighting_iw4::GFX_SPOT_SHADOW_RT_LARGE => uploaded.spot_shadow_rt10.clone(),
            lighting_iw4::GFX_SPOT_SHADOW_RT_SMALL => uploaded.spot_shadow_rt11.clone(),
            _ => None,
        }
        .ok_or(TextureBindRefusal::SpotShadowUnresolved { register })?
        .map_err(|cause| TextureBindRefusal::UploadedView { register, cause })?;
        return Ok((view, UploadedTextureIdentity::SpotShadowRt(rt)));
    }
    if index == u32::from(lighting_iw4::TEXTURE_SRC_CODE_MODEL_LIGHTING) {
        let view = uploaded
            .model_lighting
            .clone()
            .ok_or(TextureBindRefusal::ModelLightingAtlasUnavailable { register })?
            .map_err(|cause| TextureBindRefusal::UploadedView { register, cause })?;
        return Ok((view, UploadedTextureIdentity::Code(index)));
    }
    if index == u32::from(lighting_iw4::TEXTURE_SRC_CODE_LIGHT_ATTENUATION)
        || index == crate::drawsurf::CODE_TEXTURE_OUTDOOR
    {
        let id =
            catalog_image.ok_or(TextureBindRefusal::CodeImageUnproduced { register, index })?;
        let view = take_uploaded_material(uploaded, id, register, SamplerTextureDimension::D2)?;
        return Ok((view, UploadedTextureIdentity::Material(id)));
    }
    Err(TextureBindRefusal::CodeImageUnproduced { register, index })
}

fn take_uploaded_probe(
    uploaded: &RuntimeUploadedImageRegistry,
    id: SurfaceReflectionProbeId,
    register: u16,
) -> Result<TextureView, TextureBindRefusal> {
    uploaded
        .reflection_probes
        .get(usize::from(id.0))
        .and_then(Option::as_ref)
        .ok_or(TextureBindRefusal::ReflectionProbeUnavailable { register, id })?
        .clone()
        .map_err(|cause| TextureBindRefusal::UploadedView { register, cause })
}

fn take_uploaded_lightmap(
    uploaded: &RuntimeUploadedImageRegistry,
    id: SurfaceLightmapId,
    primary: bool,
    register: u16,
) -> Result<TextureView, TextureBindRefusal> {
    let page = uploaded
        .lightmaps
        .get(usize::from(id.0))
        .and_then(Option::as_ref)
        .ok_or(TextureBindRefusal::LightmapUnavailable { register, id })?;
    let view = if primary {
        &page.primary
    } else {
        &page.secondary
    };
    view.clone().map_err(|cause| match cause {
        UploadedViewRefusal::ExactImageNotRetained => {
            TextureBindRefusal::LightmapExactImageUnavailable {
                register,
                id,
                primary,
            }
        }
        other => TextureBindRefusal::UploadedView {
            register,
            cause: other,
        },
    })
}

fn material_image(
    images: &RuntimeImageHandles,
    id: RuntimeImageId,
    register: u16,
) -> Result<Handle<Image>, TextureBindRefusal> {
    images
        .material_images
        .get(id.0 as usize)
        .and_then(Clone::clone)
        .ok_or(TextureBindRefusal::MaterialImageUnavailable { register, id })
}

fn code_image(
    images: &RuntimeImageHandles,
    index: u32,
    register: u16,
    catalog_image: Option<RuntimeImageId>,
) -> Result<Handle<Image>, TextureBindRefusal> {
    if index == u32::from(lighting_iw4::TEXTURE_SRC_CODE_MODEL_LIGHTING) {
        return images
            .model_lighting
            .clone()
            .ok_or(TextureBindRefusal::ModelLightingAtlasUnavailable { register });
    }
    if index == u32::from(lighting_iw4::TEXTURE_SRC_CODE_LIGHT_ATTENUATION)
        || index == crate::drawsurf::CODE_TEXTURE_OUTDOOR
    {
        let id =
            catalog_image.ok_or(TextureBindRefusal::CodeImageUnproduced { register, index })?;
        return material_image(images, id, register);
    }
    Err(TextureBindRefusal::CodeImageUnproduced { register, index })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamplerTableBuildRefusal {
    MipModeOutOfRange {
        mip_mode: u8,
    },
    CapturedWordsMismatch {
        expected: [u32; 24],
        produced: [u32; 24],
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextureBindRefusal {
    ExecutablePairMismatch,
    ExecutableSamplerArgumentMissing {
        register: u16,
    },
    CodeImageUnproduced {
        register: u16,
        index: u32,
    },
    ModelLightingAtlasUnavailable {
        register: u16,
    },
    ResolvedPostSunUnavailable {
        register: u16,
    },
    FloatZUnresolved {
        register: u16,
    },
    SunShadowUnresolved {
        register: u16,
    },
    SpotShadowUnresolved {
        register: u16,
    },
    MaterialImageUnavailable {
        register: u16,
        id: RuntimeImageId,
    },
    StaleImageGeneration {
        retained: MaterialGenerationId,
        current: MaterialGenerationId,
    },
    SurfaceReflectionProbeMissing {
        register: u16,
    },
    SurfacePrimaryLightmapMissing {
        register: u16,
    },
    SurfaceSecondaryLightmapMissing {
        register: u16,
    },
    ReflectionProbeUnavailable {
        register: u16,
        id: SurfaceReflectionProbeId,
    },
    LightmapUnavailable {
        register: u16,
        id: SurfaceLightmapId,
    },
    LightmapExactImageUnavailable {
        register: u16,
        id: SurfaceLightmapId,
        primary: bool,
    },
    SamplerTableIndexUninitialized {
        sampler_state: u8,
        table_index: u8,
    },
    UnsupportedMinMagFilter {
        sampler_state: u8,
        packed_word: u32,
        raw: u32,
    },
    UnsupportedMipFilter {
        sampler_state: u8,
        packed_word: u32,
        raw: u32,
    },
    AnisotropyOverflow {
        sampler_state: u8,
        packed_word: u32,
        anisotropy: u32,
    },
    UploadedViewDimensionMismatch {
        register: u16,
        expected: TextureViewDimension,
        actual: TextureViewDimension,
    },
    UploadedView {
        register: u16,
        cause: UploadedViewRefusal,
    },
}
