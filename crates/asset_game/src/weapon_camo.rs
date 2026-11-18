use crate::CapturedStringTable;
use asset_core::MaterialKey;
use asset_material::MaterialCatalog;
#[derive(Clone, Debug)]
pub struct WeaponCamouflage {
    pub slot: u8,
    pub name: String,
    pub caption_key: String,
    pub preview: String,
    pub materials: Vec<(MaterialKey, MaterialKey)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponCamouflageChoice {
    pub slot: u8,
    pub name: String,
    pub caption_key: String,
    pub preview: String,
}

pub trait MaterialCamouflageCompiler {
    fn prepare(
        &mut self,
        table: &CapturedStringTable,
        choices: &CapturedStringTable,
        parent: &str,
        keys: &[MaterialKey],
        materials: &mut MaterialCatalog,
    ) -> Vec<WeaponCamouflage>;
}
#[derive(Clone, Copy)]
pub struct WeaponModelCamouflagePolicy {
    pub table: &'static str,
    pub label: &'static str,
    pub choices: fn(&CapturedStringTable, &MaterialCatalog) -> Vec<WeaponCamouflageChoice>,
}
#[derive(Clone, Copy)]
pub struct WeaponMaterialCamouflagePolicy {
    pub options: &'static str,
    pub choices: &'static str,
    pub label: &'static str,
    pub compiler: fn() -> Box<dyn MaterialCamouflageCompiler>,
    pub image_source: fn(&asset_transport::GamesRoot) -> Result<asset_transport::ZoneFile, String>,
}
#[derive(Clone, Copy)]
pub struct WeaponCamouflagePolicy {
    pub model: Option<WeaponModelCamouflagePolicy>,
    pub materials: Option<WeaponMaterialCamouflagePolicy>,
}
