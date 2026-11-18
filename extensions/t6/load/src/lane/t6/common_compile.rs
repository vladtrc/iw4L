mod effects;
mod models;
pub(super) use effects::bind_t6_fx;
use models::bind_t6_content;

use assets::{CommonFamilyCompiler, CommonPreparationProducts, CommonPreparationResult};

pub(super) struct T6CommonCompiler {
    pub content: super::T6Content,
    pub captured_weapons: usize,
}

impl CommonFamilyCompiler for T6CommonCompiler {
    fn compile(self: Box<Self>, products: CommonPreparationProducts) -> CommonPreparationResult {
        let CommonPreparationProducts {
            mut weapons,
            materials: mut material_seed,
            fpv: mut fpv_meshes,
            world: mut world_weapons,
            projectiles: mut projectile_meshes,
            mut xanims,
            fx: mut common_fx,
            mut report,
        } = products;
        let t6_captured = self.captured_weapons;
        let mut content = self.content;
        let impact_fx = content.impact_fx.take();
        let t6_hands = content.hands.clone();
        let (added, kept) = xanims.absorb_vacant(std::mem::take(&mut content.xanims));
        report.push(format!(
            "t6 xanims absorbed: +{}, {kept} names already taken",
            added.len()
        ));
        let mut refusals = Vec::new();
        report.push(bind_t6_fx(
            &mut content,
            &mut material_seed,
            &mut common_fx,
            &mut refusals,
        ));
        let camouflages = std::mem::take(&mut content.camouflages);
        report.push(bind_t6_content(
            content,
            &mut material_seed,
            &mut fpv_meshes,
            &mut world_weapons,
            &mut refusals,
        ));
        let dressed = weapons.set_material_camouflages(crate::ID, camouflages, &material_seed);
        report.push(format!(
            "T6 camouflage: {dressed} weapon configurations prepared"
        ));
        refusals.extend(weapons.prepare_family_models(
            &fpv_meshes,
            &asset_game::WeaponModelPreparation {
                family: crate::ID,
                hands: t6_hands.as_deref(),
                notetrack_convention: asset_game::NotetrackConvention::InlinePrefix,
            },
        ));
        let mut t6_projectiles = 0usize;
        for id in 1..=weapons.len() as u32 {
            if weapons.identity_namespace_of(id) != Some(crate::ID) {
                continue;
            }
            let Some(name) = weapons.projectile_model_of(id) else {
                continue;
            };
            if !projectile_meshes.contains(crate::ID, name)
                && let Some(gun) = world_weapons.get(crate::ID, name)
            {
                projectile_meshes.absorb_world_weapon(gun);
                t6_projectiles += 1;
            }
        }
        report.push(format!(
            "t6 native weapons: captured={t6_captured}, projectiles={t6_projectiles}"
        ));

        CommonPreparationResult {
            products: CommonPreparationProducts {
                weapons,
                materials: material_seed,
                fpv: fpv_meshes,
                world: world_weapons,
                projectiles: projectile_meshes,
                xanims,
                fx: common_fx,
                report,
            },
            impact_fx,
            refusals,
        }
    }
}
