use bevy::prelude::*;
use fx::{FxMsec, FxSystemHost, PlayResult, SpawnFail, axis_from_hit_normal};
use fx_iw4::{
    FX_IMPACT_EXIT_SURFACE_FLAG, FX_SURF_TYPE_FLESH, flesh_effect_index, impact_table_row,
};
use net::CEntitySlots;
use weapon_iw4::SURFACE_TYPE_NAMES;

use crate::present::{
    FxElemInfoCache, FxScene, play_named_bolted_in_world, play_named_oriented_in_world,
};
use crate::{
    CombatFxDump, FireFxOccurrence, FireFxOutcome, FireFxRequest, FxJournalCursor, HostFxSystem,
    PreparedFxCatalog, PreparedImpactFx, PreparedTracers, PresentedFireFx, TracerDrawGate,
    TracerPellet, TracerSegment, TracerSpawnSkip, TracerWorld,
};

pub const IDENTITY_AXIS: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

pub fn try_play_weapon_fx_at_origin(
    host: &mut FxSystemHost,
    catalog: &asset_game::FxDefinitions,
    cache: &mut FxElemInfoCache,
    name: Option<asset_game::FxName<'_>>,
    orientation: fx_iw4::FxOrientation,
    played: &mut u32,
    scene: Option<&dyn FxScene>,
) -> bool {
    let Some(name) = name else {
        return false;
    };
    cache.sync(catalog);
    match play_named_oriented_in_world(
        host,
        catalog,
        cache,
        name,
        orientation.origin,
        orientation.axis,
        scene,
    ) {
        Some(PlayResult::PlayedReleased { .. } | PlayResult::Held { .. }) => {
            *played = played.saturating_add(1);
            true
        }
        Some(PlayResult::Failed(_)) | None => false,
    }
}

pub fn present_weapon_fx_bolted(
    host: &mut FxSystemHost,
    catalog: &asset_game::FxDefinitions,
    cache: &mut FxElemInfoCache,
    name: Option<asset_game::FxName<'_>>,
    target: Option<fx::FxBoltTarget>,
    played: &mut u32,
    scene: Option<&dyn FxScene>,
) -> FireFxOutcome {
    let Some(name) = name else {
        return FireFxOutcome::DependencyRefused("weapon_fx_definition");
    };
    let Some(target) = target else {
        return FireFxOutcome::DependencyRefused("weapon_fx_bolt");
    };
    cache.sync(catalog);
    match play_named_bolted_in_world(host, catalog, cache, name, target, scene) {
        Some(PlayResult::PlayedReleased { .. } | PlayResult::Held { .. }) => {
            *played = played.saturating_add(1);
            FireFxOutcome::Created(1)
        }
        Some(PlayResult::Failed(reason)) => FireFxOutcome::RetryableFailure(reason),
        None => FireFxOutcome::DependencyRefused("weapon_fx_catalog"),
    }
}

pub fn try_play_weapon_fx_bolted(
    host: &mut FxSystemHost,
    catalog: &asset_game::FxDefinitions,
    cache: &mut FxElemInfoCache,
    name: Option<asset_game::FxName<'_>>,
    target: Option<fx::FxBoltTarget>,
    played: &mut u32,
    scene: Option<&dyn FxScene>,
) -> bool {
    matches!(
        present_weapon_fx_bolted(host, catalog, cache, name, target, played, scene),
        FireFxOutcome::Created(_)
    )
}

#[derive(Clone, Copy, Debug)]
pub struct ShellEjectRequest<'a> {
    pub combat_fx: Option<&'a asset_game::WeaponCombatFx>,
    pub player_view: bool,
    pub last_shot: bool,
    pub target: Option<fx::FxBoltTarget>,
}

impl ShellEjectRequest<'_> {
    pub fn present(
        &self,
        host: &mut FxSystemHost,
        catalog: &asset_game::FxDefinitions,
        cache: &mut FxElemInfoCache,
        cursor: &mut FxJournalCursor,
        combat: &mut CombatFxDump,
        scene: Option<&dyn FxScene>,
    ) -> FireFxOutcome {
        combat.last_brass_lastshot = Some(i64::from(
            self.last_shot
                && self
                    .combat_fx
                    .is_some_and(|fx| fx.last_shot_eject_pair_authored()),
        ));
        let name = self
            .combat_fx
            .and_then(|fx| fx.brass_present_for_event(self.player_view, self.last_shot));
        combat.last_weapon_brass_edge = self.combat_fx.map(|fx| {
            fx.brass_edge_for_event(self.player_view, self.last_shot)
                .edge_kind()
                .to_owned()
        });
        if let Some(name) = name {
            combat.last_brass_name = Some(name.name.to_owned());
        }
        let outcome = present_weapon_fx_bolted(
            host,
            catalog,
            cache,
            name,
            self.target,
            &mut cursor.brass_played,
            scene,
        );
        if !matches!(outcome, FireFxOutcome::Created(_)) {
            cursor.brass_gap = cursor.brass_gap.saturating_add(1);
        }
        outcome
    }
}

pub fn missile_bolt_target(
    poses: Option<&render_anim::HostDObjPoseFrame>,
    meshes: &asset_model::ProjectileMeshCatalog,
    namespace: asset_core::FamilyId,
    model: &str,
    entnum: u32,
) -> Option<fx::FxBoltTarget> {
    let entry = meshes.get(namespace, model)?;
    let bone = entry
        .skel
        .bone_names
        .iter()
        .position(|name| name == "tag_fx")?;
    let bone = u16::try_from(bone).ok()?;
    let resolved = poses?.resolve_live_bolt(entnum, bone)?;
    let orientation = resolved.orientation?;
    Some(fx::FxBoltTarget {
        dobj: entnum,
        bone,
        centity_teleport: resolved.centity_teleport,
        orientation,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExplosionFxNames<'a> {
    pub table: Option<asset_game::FxName<'a>>,
    pub slot: Option<asset_game::FxName<'a>>,
    pub row: Option<usize>,
}

pub fn explosion_fx_names<'a>(
    impact_type: Option<i32>,
    surf_type: u8,
    table: Option<&'a asset_game::OwnedFxImpactTable>,
    slot: Option<asset_game::FxName<'a>>,
) -> ExplosionFxNames<'a> {
    let row = impact_type.and_then(|t| {
        table.map_or_else(
            || impact_table_row(t, false),
            |table| table.impact_row(t, false),
        )
    });
    let surf = surf_type as usize;
    let flesh = (surf == FX_SURF_TYPE_FLESH).then_some(0);
    let table = row.and_then(|row| table.and_then(|t| t.effect_name(row, surf, flesh)));
    let table = table.filter(|name| slot.is_none_or(|slot| slot.namespace == name.namespace));
    let slot = slot.filter(|name| Some(*name) != table);
    ExplosionFxNames { table, slot, row }
}

#[derive(bevy::ecs::system::SystemParam)]
struct ImpactFxPresentation<'w> {
    catalog: Option<Res<'w, PreparedFxCatalog>>,
    elem_infos: ResMut<'w, crate::PreparedFxElemInfos>,
    impact_fx: Option<Res<'w, PreparedImpactFx>>,
    host: ResMut<'w, HostFxSystem>,
    cursor: ResMut<'w, FxJournalCursor>,
    combat: ResMut<'w, CombatFxDump>,
}

#[derive(bevy::ecs::system::SystemParam)]
pub struct PelletFxPresentation<'w, 's> {
    level: Option<Res<'w, net::ClientLevel>>,
    world_bolts: Query<'w, 's, &'static render_anim::RemoteFxBolts>,
    fpv_bolts: Res<'w, render_anim::FpvBoltTargets>,
    slots: Res<'w, CEntitySlots>,
    weapons: Option<Res<'w, assets::PreparedWeapons>>,
    tracers: Option<Res<'w, PreparedTracers>>,
    tracer_world: ResMut<'w, TracerWorld>,
    gate: ResMut<'w, TracerDrawGate>,
    local: Res<'w, net::LocalPresentClient>,
    occurrences: ResMut<'w, PresentedFireFx>,
    impact: ImpactFxPresentation<'w>,
}

impl PelletFxPresentation<'_, '_> {
    pub fn now(&self) -> i32 {
        self.impact.host.0.msec_now
    }

    pub fn local_number(&self) -> i32 {
        self.local.0.0 as i32
    }

    pub fn present(
        &mut self,
        request: FireFxRequest,
        record: &sim::PelletFxRecord,
        target: Option<u16>,
        scene: Option<&dyn FxScene>,
    ) {
        let mut placed = *record;
        if let Some(atlas) = self.level.as_ref().and_then(|level| level.content.atlas()) {
            let world = asset_core::atlas::WorldLayout.world_at(record.start);
            placed.start = atlas.space.simulation_point_in(world, record.start);
            placed.end = atlas.space.simulation_point_in(world, record.end);
            placed.normal = atlas.space.simulation_vector(record.start, record.normal);
        }
        let record = &placed;
        // Deduplicated events still publish the mutable presentation resources as changed.
        self.occurrences.set_changed();
        self.tracer_world.set_changed();
        self.gate.set_changed();
        self.impact.elem_infos.set_changed();
        self.impact.host.set_changed();
        self.impact.cursor.set_changed();
        self.impact.combat.set_changed();
        let Self {
            level: _,
            world_bolts,
            fpv_bolts,
            slots,
            weapons,
            tracers,
            tracer_world,
            gate,
            local,
            occurrences,
            impact,
        } = self;
        let bound = weapons.as_deref().map(|weapons| weapons.bound());
        let weapons = bound.as_ref();
        let tracers = tracers.as_deref();
        let local_number = local.0.0 as i32;
        let sim::PelletFxRecord {
            attacker: attacker_entity_num,
            weapon,
            correlation,
            pellet,
            segment,
            hand,
            start: seg_start,
            end: seg_end,
            normal,
            surf_type,
            surface_flags,
            ..
        } = *record;
        let mut request = request;
        request.occurrence = FireFxOccurrence::Glass { pellet, segment };
        occurrences.execute(request, || {
            FireFxOutcome::Applied(impact.host.0.glass.hit_segment(
                seg_start,
                seg_end,
                (u64::from(correlation) << 32)
                    ^ (u64::from(attacker_entity_num as u32) << 16)
                    ^ u64::from(pellet),
            ) as u32)
        });
        request.occurrence = FireFxOccurrence::Tracer { pellet, segment };
        let tracer = occurrences.execute(request, || {
            impact.combat.last_surf = Some(i64::from(surf_type));
            impact.combat.last_surf_flags = Some(i64::from(surface_flags));
            impact.combat.last_surf_name = SURFACE_TYPE_NAMES
                .get(surf_type as usize)
                .map(|name| (*name).to_owned());
            let tracer_edge = weapons
                .and_then(|weapons| weapons.registry().combat_fx_of(weapon))
                .map(|fx| fx.tracer)
                .unwrap_or(assets::AssetEdge::Absent);
            let own_shot = attacker_entity_num == local_number;
            let source_id = attacker_entity_num.max(0) as u32;
            let tracer_pellet = TracerPellet {
                cause: request.cause,
                source_id,
                correlation,
                pellet,
            };

            let tag_start = gate
                .first_segment_of_pellet(tracer_pellet, segment, request.now)
                .then(|| {
                    if own_shot {
                        fpv_bolts.flash[usize::from(hand).min(fpv_bolts.flash.len() - 1)]
                    } else {
                        u16::try_from(attacker_entity_num)
                            .ok()
                            .and_then(|number| slots.entity_for_number(number))
                            .and_then(|entity| world_bolts.get(entity).ok())
                            .and_then(|bolts| bolts.flash)
                    }
                })
                .flatten()
                .map(|target| target.orientation.origin);
            let start = tag_start.unwrap_or(seg_start);
            match tracers.map(|t| {
                tracer_world.try_spawn(
                    gate,
                    &t.0,
                    tracer_edge,
                    TracerSegment {
                        pellet: tracer_pellet,
                        start,
                        end: seg_end,
                        own_shot,
                    },
                    FxMsec::from_host(&impact.host.0),
                    &mut impact.combat,
                )
            }) {
                None | Some(Err(TracerSpawnSkip::NoDef)) => {
                    impact.cursor.tracer_gap = impact.cursor.tracer_gap.saturating_add(1);
                    impact.cursor.tracer_skip_no_def =
                        impact.cursor.tracer_skip_no_def.saturating_add(1);
                    FireFxOutcome::DependencyRefused("tracer_definition")
                }
                Some(Err(TracerSpawnSkip::Interval)) => {
                    impact.cursor.tracer_gap = impact.cursor.tracer_gap.saturating_add(1);
                    impact.cursor.tracer_skip_interval =
                        impact.cursor.tracer_skip_interval.saturating_add(1);
                    FireFxOutcome::PolicySkipped("tracer_interval")
                }
                Some(Err(TracerSpawnSkip::HistoryCapacity)) => {
                    FireFxOutcome::HistoryCapacityRefused
                }
                Some(Err(TracerSpawnSkip::Short)) => {
                    impact.cursor.tracer_gap = impact.cursor.tracer_gap.saturating_add(1);
                    impact.cursor.tracer_skip_short =
                        impact.cursor.tracer_skip_short.saturating_add(1);
                    FireFxOutcome::NotApplicable("tracer_short")
                }
                Some(Ok(())) => {
                    impact.combat.tracer_spawned = impact.combat.tracer_spawned.saturating_add(1);
                    if tag_start.is_some() {
                        impact.cursor.tracer_from_tag =
                            impact.cursor.tracer_from_tag.saturating_add(1);
                    }
                    FireFxOutcome::Created(1)
                }
            }
        });
        let mut play_impact = |products| {
            if normal == [0.0, 0.0, 0.0] {
                sync_combat_dump(&impact.cursor, &mut impact.combat);
                return FireFxOutcome::NotApplicable("no_hit_normal");
            }
            let Some(impact_type) = weapons
                .and_then(|w| w.row(weapon).and_then(|weapon| weapon.event_facts()))
                .map(|f| f.impact_type)
            else {
                impact.cursor.impact_miss_table = impact.cursor.impact_miss_table.saturating_add(1);
                impact.combat.last_impact_miss_why = Some("no_weapon".into());
                sync_combat_dump(&impact.cursor, &mut impact.combat);
                return FireFxOutcome::DependencyRefused("weapon");
            };
            let previous_mark_entity = impact.host.0.spawn_mark_entity;
            let previous_products = impact.host.0.spawn_products;
            impact.host.0.spawn_products = products;
            impact.host.0.spawn_mark_entity = target;
            let outcome = impact.play(products, impact_type, record, scene);
            impact.host.0.spawn_mark_entity = previous_mark_entity;
            impact.host.0.spawn_products = previous_products;
            outcome
        };
        request.occurrence = FireFxOccurrence::Impact { pellet, segment };
        let impact_outcome =
            occurrences.execute(request, || play_impact(fx::FxSpawnProducts::Transient));
        request.occurrence = FireFxOccurrence::Marks { pellet, segment };
        occurrences.execute(request, || play_impact(fx::FxSpawnProducts::Marks));
        if [tracer, impact_outcome]
            .iter()
            .any(|outcome| matches!(outcome, FireFxOutcome::Created(_)))
        {
            impact.cursor.pellet_played = impact.cursor.pellet_played.saturating_add(1);
        }
        sync_combat_dump(&impact.cursor, &mut impact.combat);
    }
}

impl ImpactFxPresentation<'_> {
    fn play(
        &mut self,
        products: fx::FxSpawnProducts,
        impact_type: i32,
        record: &sim::PelletFxRecord,
        scene: Option<&dyn FxScene>,
    ) -> FireFxOutcome {
        let origin = record.end;
        let normal = record.normal;
        let surf_type = record.surf_type;
        let surface_flags = record.surface_flags;
        let flesh_flags = u32::from(record.flesh_flags);
        let catalog = self.catalog.as_deref();
        let impact_fx = self.impact_fx.as_deref();
        let cache = &mut self.elem_infos.0;
        let host = &mut *self.host;
        let cursor = &mut *self.cursor;
        let combat = &mut *self.combat;
        let previous_products = host.0.spawn_products;
        host.0.spawn_products = products;
        let outcome = (|| {
            combat.last_surf = Some(i64::from(surf_type));
            combat.last_surf_name = SURFACE_TYPE_NAMES
                .get(surf_type as usize)
                .map(|name| (*name).to_owned());
            let exit = surface_flags & FX_IMPACT_EXIT_SURFACE_FLAG != 0;
            let table = impact_fx.and_then(|fx| fx.map.as_ref());
            let Some(row) = table.map_or_else(
                || impact_table_row(impact_type, exit),
                |table| table.impact_row(impact_type, exit),
            ) else {
                cursor.impact_miss_table = cursor.impact_miss_table.saturating_add(1);
                combat.last_impact_miss_why = Some("no_row".into());
                sync_combat_dump(cursor, combat);
                return FireFxOutcome::DependencyRefused("impact_row");
            };
            combat.last_row = Some(row as i64);
            let surf = surf_type as usize;
            let flesh_slot = (surf == FX_SURF_TYPE_FLESH).then(|| flesh_effect_index(flesh_flags));
            combat.last_impact_cell_empty = table.and_then(|t| {
                let entry = t.entries.get(row)?;
                if let Some(slot) = flesh_slot {
                    return Some(i64::from(
                        entry.flesh.get(slot).is_some_and(|name| name.is_empty()),
                    ));
                }
                if surf >= fx_iw4::FX_IMPACT_NONFLESH_COUNT {
                    return Some(0);
                }
                Some(i64::from(entry.nonflesh[surf].is_empty()))
            });
            if combat.last_impact_cell_empty == Some(1) {
                combat.last_impact_miss_why = None;
                sync_combat_dump(cursor, combat);
                return FireFxOutcome::NotApplicable("empty_impact_cell");
            }
            let Some(def_name) = table.and_then(|t| t.effect_name(row, surf, flesh_slot)) else {
                cursor.impact_miss_def = cursor.impact_miss_def.saturating_add(1);
                combat.last_impact_miss_why = Some(
                    if table.is_none() {
                        "no_table"
                    } else {
                        "cell_miss"
                    }
                    .into(),
                );
                if !cursor.impact_miss_def_warned {
                    cursor.impact_miss_def_warned = true;
                    diag::warn!(
                        World,
                        "fx: BulletImpact row={row} surf={surf} — no ImpactFx cell name; play_oriented skipped (further misses counted)"
                    );
                }
                sync_combat_dump(cursor, combat);
                return FireFxOutcome::DependencyRefused("impact_table_cell");
            };
            combat.last_impact_def = Some(def_name.name.to_owned());
            combat.last_impact_miss_why = None;
            let Some(catalog) = catalog else {
                combat.last_impact_miss_why = Some("no_catalog".into());
                return FireFxOutcome::DependencyRefused("fx_catalog");
            };
            if products == fx::FxSpawnProducts::Marks
                && def_name
                    .resolve(&catalog.0)
                    .is_some_and(|effect| !catalog.0.can_emit_marks(effect))
            {
                return FireFxOutcome::NotApplicable("no_mark_product");
            }
            cache.sync(&catalog.0);
            let Some(result) = play_named_oriented_in_world(
                &mut host.0,
                &catalog.0,
                cache,
                def_name,
                origin,
                axis_from_hit_normal(normal),
                scene,
            ) else {
                cursor.impact_miss_def = cursor.impact_miss_def.saturating_add(1);
                combat.last_impact_miss_why = Some("catalog".into());
                if !cursor.impact_miss_def_warned {
                    cursor.impact_miss_def_warned = true;
                    diag::warn!(
                        World,
                        "fx: BulletImpact cell `{def_name}` not in FxCatalog — play_oriented skipped"
                    );
                }
                sync_combat_dump(cursor, combat);
                return FireFxOutcome::DependencyRefused("impact_definition");
            };
            if matches!(
                result,
                PlayResult::PlayedReleased { .. } | PlayResult::Held { .. }
            ) {
                if products != fx::FxSpawnProducts::Marks {
                    cursor.impact_played = cursor.impact_played.saturating_add(1);
                    combat.impact_msec = Some(host.0.msec_now);
                }
            } else if let PlayResult::Failed(e) = result {
                cursor.impact_miss_def = cursor.impact_miss_def.saturating_add(1);
                combat.last_impact_miss_why = Some(
                    match e {
                        SpawnFail::RingFull => "ring_full",
                        SpawnFail::TooManySpotlights => "spotlights",
                        SpawnFail::EffectLimit => "effect_limit",
                    }
                    .into(),
                );
                diag::warn!(World, "fx: play_oriented `{def_name}` failed: {e:?}");
            }

            log_combat_fx_gaps(cursor, combat);
            sync_combat_dump(cursor, combat);
            match result {
                PlayResult::PlayedReleased { handle } | PlayResult::Held { handle } => {
                    if products == fx::FxSpawnProducts::Marks {
                        FireFxOutcome::Scheduled(u32::from(handle))
                    } else {
                        FireFxOutcome::Created(u32::from(handle))
                    }
                }
                PlayResult::Failed(reason) => FireFxOutcome::RetryableFailure(reason),
            }
        })();
        host.0.spawn_products = previous_products;
        outcome
    }
}

pub fn log_combat_fx_gaps(cursor: &mut FxJournalCursor, combat: &CombatFxDump) {
    if !cursor.logged
        && (cursor.muzzle_played > 0
            || cursor.muzzle_gap > 0
            || combat.tracer_spawned > 0
            || cursor.tracer_gap > 0
            || cursor.brass_played > 0
            || cursor.pellet_played > 0
            || cursor.impact_miss_def > 0
            || cursor.boom_played > 0
            || cursor.explosion_gap > 0
            || cursor.impact_played > 0)
    {
        diag::info!(
            World,
            "fx: muzzle_gap={} muzzle_played={} muzzle_bolted={} tracer_gap={} tracer_spawned={} tracer_from_tag={} tracer_skip_interval={} tracer_skip_short={} tracer_skip_no_def={} fire_sound_gap={} explosion_sound_gap={} explosion_gap={} brass_gap={} brass_played={} pellet_played={} impact_played={} impact_miss_table={} impact_miss_def={} boom_played={}",
            cursor.muzzle_gap,
            cursor.muzzle_played,
            cursor.muzzle_bolted,
            cursor.tracer_gap,
            combat.tracer_spawned,
            cursor.tracer_from_tag,
            cursor.tracer_skip_interval,
            cursor.tracer_skip_short,
            cursor.tracer_skip_no_def,
            cursor.fire_sound_gap,
            cursor.explosion_sound_gap,
            cursor.explosion_gap,
            cursor.brass_gap,
            cursor.brass_played,
            cursor.pellet_played,
            cursor.impact_played,
            cursor.impact_miss_table,
            cursor.impact_miss_def,
            cursor.boom_played
        );
        cursor.logged = true;
    }
}

pub fn sync_combat_dump(cursor: &FxJournalCursor, combat: &mut CombatFxDump) {
    combat.muzzle_gap = cursor.muzzle_gap;
    combat.muzzle_played = cursor.muzzle_played;
    combat.muzzle_bolted = cursor.muzzle_bolted;
    combat.tracer_from_tag = cursor.tracer_from_tag;
    combat.tracer_gap = cursor.tracer_gap;
    combat.brass_gap = cursor.brass_gap;
    combat.brass_played = cursor.brass_played;
    combat.pellet_played = cursor.pellet_played;
    combat.explosion_played = cursor.boom_played;
    combat.explosion_gap = cursor.explosion_gap;
    combat.impact_played = cursor.impact_played;
    combat.impact_miss_table = cursor.impact_miss_table;
    combat.impact_miss_def = cursor.impact_miss_def;
    combat.tracer_skip_interval = cursor.tracer_skip_interval;
    combat.tracer_skip_short = cursor.tracer_skip_short;
    combat.tracer_skip_no_def = cursor.tracer_skip_no_def;
    combat.fire_sound_gap = cursor.fire_sound_gap;
}
