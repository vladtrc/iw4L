use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{
    Extent3d, TextureDimension, TextureFormat, TextureViewDescriptor, TextureViewDimension,
};
use lighting_iw4::{
    MODEL_LIGHTING_ATLAS_DEPTH, MODEL_LIGHTING_ATLAS_WIDTH, MODEL_LIGHTING_TILE_BYTES,
    ModelLightingAtlasDims, ModelLightingTileIndex, model_lighting_write_tile_to_atlas,
};

#[derive(Resource)]
pub struct WorldModelLightingAtlas {
    pub image: Handle<Image>,
    pub dims: ModelLightingAtlasDims,
}

impl WorldModelLightingAtlas {
    pub fn new(images: &mut Assets<Image>, dims: ModelLightingAtlasDims) -> Self {
        Self {
            image: images.add(model_lighting_atlas_image(dims)),
            dims,
        }
    }
}

pub fn model_lighting_atlas_image(dims: ModelLightingAtlasDims) -> Image {
    let width = MODEL_LIGHTING_ATLAS_WIDTH;
    let depth = MODEL_LIGHTING_ATLAS_DEPTH;
    let bytes = (width * dims.image_height * depth * 4) as usize;
    let mut image = Image::new(
        Extent3d {
            width,
            height: dims.image_height,
            depth_or_array_layers: depth,
        },
        TextureDimension::D3,
        vec![0u8; bytes],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_view_descriptor = Some(TextureViewDescriptor {
        dimension: Some(TextureViewDimension::D3),
        ..default()
    });
    image
}

pub fn model_lighting_atlas_write_tile(
    atlas: &mut Image,
    dims: ModelLightingAtlasDims,
    entry: ModelLightingTileIndex,
    tile: &[u8; MODEL_LIGHTING_TILE_BYTES],
) -> bool {
    if atlas.texture_descriptor.size.width != MODEL_LIGHTING_ATLAS_WIDTH
        || atlas.texture_descriptor.size.height != dims.image_height
        || atlas.texture_descriptor.size.depth_or_array_layers != MODEL_LIGHTING_ATLAS_DEPTH
    {
        return false;
    }
    let Some(data) = atlas.data.as_mut() else {
        return false;
    };
    model_lighting_write_tile_to_atlas(
        data,
        dims.image_height,
        entry,
        &model_lighting_tile_texels(tile),
    )
}

pub fn model_lighting_tile_texels(
    tile: &[u8; MODEL_LIGHTING_TILE_BYTES],
) -> [u8; MODEL_LIGHTING_TILE_BYTES] {
    let Some(flat) = ml_tile_debug_colour() else {
        return *tile;
    };
    let mut painted = [0u8; MODEL_LIGHTING_TILE_BYTES];
    for texel in painted.as_chunks_mut::<4>().0 {
        texel.copy_from_slice(&flat);
    }
    painted
}

#[derive(Resource, Default)]
pub struct ModelLightingAtlasTileWrites {
    pub tiles: Vec<ModelLightingAtlasTileWrite>,
}

pub struct ModelLightingAtlasTileWrite {
    pub image: AssetId<Image>,
    pub origin: (u32, u32),
    pub texels: [u8; MODEL_LIGHTING_TILE_BYTES],
}

impl ModelLightingAtlasTileWrites {
    pub fn push(
        &mut self,
        image: AssetId<Image>,
        entry: ModelLightingTileIndex,
        tile: &[u8; MODEL_LIGHTING_TILE_BYTES],
    ) {
        self.tiles.push(ModelLightingAtlasTileWrite {
            image,
            origin: entry.texel_origin(),
            texels: model_lighting_tile_texels(tile),
        });
    }
}

fn ml_tile_debug_colour() -> Option<[u8; 4]> {
    use std::sync::OnceLock;
    static COLOUR: OnceLock<Option<[u8; 4]>> = OnceLock::new();
    *COLOUR.get_or_init(|| {
        let raw = std::env::var("IW4L_ML_TILE_DEBUG").ok()?;
        let mut parts = raw.split(',').map(|part| part.trim().parse::<u8>());
        let (Some(Ok(r)), Some(Ok(g)), Some(Ok(b))) = (parts.next(), parts.next(), parts.next())
        else {
            diag::warn!(
                World,
                "unparseable IW4L_ML_TILE_DEBUG={raw:?}; tiles unchanged"
            );
            return None;
        };
        diag::warn!(
            World,
            "model lighting tile A/B: every tile forced to {r},{g},{b},255"
        );
        Some([r, g, b, 255])
    })
}
