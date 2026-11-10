use super::definitions::{
    AssetPointerIdentity, MaterialLinkAddress, OwnedTechnique, TechniqueSetFacts,
};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub enum LinkKind {
    Material,
    Image,
    Techset,
    VertexShader,
    PixelShader,
    VertexDecl,
}

#[derive(Clone, Copy, Debug)]
enum Link {
    Direct(usize),
    Alias(AssetPointerIdentity),
}

#[derive(Clone, Debug, Default)]
pub struct ZoneLinkState {
    materials: HashMap<AssetPointerIdentity, Link>,
    images: HashMap<AssetPointerIdentity, Link>,
    techsets: HashMap<AssetPointerIdentity, Link>,
    vertex_shaders: HashMap<AssetPointerIdentity, Link>,
    pixel_shaders: HashMap<AssetPointerIdentity, Link>,
    vertex_decls: HashMap<AssetPointerIdentity, Link>,
    technique_bodies: HashMap<AssetPointerIdentity, OwnedTechnique>,
    last_technique_set: Option<TechniqueSetFacts>,
}

impl ZoneLinkState {
    pub fn begin_zone(&mut self) {
        *self = Self::default();
    }

    fn map(&self, kind: LinkKind) -> &HashMap<AssetPointerIdentity, Link> {
        match kind {
            LinkKind::Material => &self.materials,
            LinkKind::Image => &self.images,
            LinkKind::Techset => &self.techsets,
            LinkKind::VertexShader => &self.vertex_shaders,
            LinkKind::PixelShader => &self.pixel_shaders,
            LinkKind::VertexDecl => &self.vertex_decls,
        }
    }

    fn map_mut(&mut self, kind: LinkKind) -> &mut HashMap<AssetPointerIdentity, Link> {
        match kind {
            LinkKind::Material => &mut self.materials,
            LinkKind::Image => &mut self.images,
            LinkKind::Techset => &mut self.techsets,
            LinkKind::VertexShader => &mut self.vertex_shaders,
            LinkKind::PixelShader => &mut self.pixel_shaders,
            LinkKind::VertexDecl => &mut self.vertex_decls,
        }
    }

    pub fn resolve(&self, kind: LinkKind, slot: impl Into<AssetPointerIdentity>) -> Option<usize> {
        let mut slot = slot.into();
        for _ in 0..32 {
            match self.map(kind).get(&slot).copied()? {
                Link::Direct(index) => return Some(index),
                Link::Alias(target) => slot = target,
            }
        }
        None
    }

    fn bind(&mut self, kind: LinkKind, slot: MaterialLinkAddress, link: Link) {
        if slot.aliasable {
            self.map_mut(kind).insert(slot.identity, link);
        }
    }

    pub fn bind_direct(
        &mut self,
        kind: LinkKind,
        slot: impl Into<MaterialLinkAddress>,
        index: usize,
    ) {
        self.bind(kind, slot.into(), Link::Direct(index));
    }

    pub fn bind_alias(
        &mut self,
        kind: LinkKind,
        slot: impl Into<MaterialLinkAddress>,
        target: impl Into<AssetPointerIdentity>,
    ) {
        self.bind(kind, slot.into(), Link::Alias(target.into()));
    }

    pub fn select_techset(&mut self, facts: Option<TechniqueSetFacts>) {
        self.last_technique_set = facts;
    }

    pub fn take_techset(&mut self) -> Option<TechniqueSetFacts> {
        self.last_technique_set.take()
    }

    pub fn technique(&self, body: impl Into<AssetPointerIdentity>) -> Option<OwnedTechnique> {
        self.technique_bodies.get(&body.into()).cloned()
    }

    pub fn remember_technique(
        &mut self,
        body: impl Into<AssetPointerIdentity>,
        technique: OwnedTechnique,
    ) {
        self.technique_bodies.insert(body.into(), technique);
    }
}
