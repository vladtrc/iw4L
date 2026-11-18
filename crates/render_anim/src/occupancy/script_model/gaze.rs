use super::*;

pub(super) struct ClientGaze {
    actor: sim::EntityRef,
    pub controller: xmodel_runtime::ActorGazeController,
    pub pose: Option<xmodel_runtime::ActorGazePose>,
    pub bones: Option<Vec<Mat4>>,
    status: Option<&'static str>,
}

fn generation_matches(snapshot: &sim::Snapshot, number: u16, generation: u32) -> bool {
    snapshot.meta.generations.get(usize::from(number)) == Some(&generation)
}

#[derive(SystemParam)]
pub(super) struct Inputs<'w, 's> {
    slots: Res<'w, net::CEntitySlots>,
    runtimes: Query<'w, 's, (&'static net::CEntity, &'static net::CEntityRuntime)>,
    level: Res<'w, net::ClientLevel>,
    presented: Res<'w, net::PresentedSnapshot>,
    clock: Res<'w, net::FrameClock>,
    xanims: Option<Res<'w, assets::PreparedXAnims>>,
    catalog: Option<Res<'w, asset_world::MapXModelSceneCatalog>>,
    prepared: Res<'w, crate::anim::model_materials::PreparedModelMaterials>,
}

impl Inputs<'_, '_> {
    fn origin(&self, gaze: &sim::ActorGaze, at_ms: i32) -> Option<Vec3> {
        let snapshot = self.presented.snapshot()?;
        if !generation_matches(snapshot, gaze.target_number, gaze.target_generation) {
            return None;
        }
        let origin = if let Some(player) = self
            .presented
            .player(sim::ClientId(gaze.target_number as u32))
        {
            let mut origin = player.origin;
            origin[2] += player.view_height_current;
            origin
        } else {
            if let Some(entity) = self.slots.entity_for_number(gaze.target_number) {
                let (identity, runtime) = self.runtimes.get(entity).ok()?;
                if identity.generation() != gaze.target_generation || !runtime.in_next_snap() {
                    return None;
                }
                runtime.pose_at(at_ms).0
            } else {
                gaze.target_origin?
            }
        };
        Some(
            self.level
                .content
                .simulation_model_transform(origin, [0.0; 3])?
                .w_axis
                .truncate(),
        )
    }
}

pub(super) fn update(
    inputs: Inputs,
    owners: Query<(Entity, &WorldScriptModelInstance, &Transform, &Visibility)>,
    mut persist: ResMut<ScriptModelDobjs>,
) {
    let Some(snapshot) = inputs.presented.snapshot() else {
        return;
    };
    if snapshot.meta.actor_gazes.is_empty()
        && persist.by_id.values().all(|slot| slot.gaze.is_none())
    {
        return;
    }
    let Some(catalog) = inputs.catalog.as_deref() else {
        return;
    };
    let at_ms = inputs
        .presented
        .entity_pose_time_ms(if inputs.clock.started() {
            inputs.clock.time()
        } else {
            sim::level_time_ms(snapshot.tick)
        });
    let live_owners: HashSet<_> = owners
        .iter()
        .filter_map(|(_, owner, _, _)| owner.authority_owner?.script_model().map(|id| id.to_wire()))
        .collect();
    for (id, slot) in &mut persist.by_id {
        if !live_owners.contains(id) {
            slot.gaze = None;
        }
    }
    let mut targets = HashMap::new();
    for gaze in &snapshot.meta.actor_gazes {
        if !generation_matches(
            snapshot,
            gaze.actor.number() as u16,
            gaze.actor.generation(),
        ) {
            continue;
        }
        let target = if gaze.target_is_actor {
            actor_eye(&inputs, &owners, &mut persist, gaze)
        } else {
            inputs.origin(gaze, at_ms)
        };
        targets.insert(gaze.actor.number(), (gaze.actor, target));
    }
    for (_, owner, transform, _) in &owners {
        let (Some(number), Some(id)) = (
            owner.gentity_number,
            owner.authority_owner.and_then(|owner| owner.script_model()),
        ) else {
            continue;
        };
        let wanted = targets.get(&i32::from(number));
        let id = id.to_wire();
        if wanted.is_some()
            && compose_or_reuse_script_dobj(
                &mut persist,
                id,
                catalog,
                &owner.dobj_state,
                &inputs.prepared,
            )
            .is_none()
        {
            continue;
        }
        let Some(slot) = persist.by_id.get_mut(&id) else {
            continue;
        };
        if let Some((actor, _)) = wanted
            && slot.gaze.as_ref().is_none_or(|state| state.actor != *actor)
        {
            slot.gaze = Some(ClientGaze {
                actor: *actor,
                controller: Default::default(),
                pose: None,
                bones: None,
                status: None,
            });
        }
        let Some(state) = &mut slot.gaze else {
            continue;
        };
        if !generation_matches(snapshot, number, state.actor.generation()) {
            slot.gaze = None;
            continue;
        }
        let Ok(request) = owner.dobj_state.resolve_request(|name| {
            inputs
                .xanims
                .as_ref()?
                .0
                .clip(asset_core::FamilyId::IW4, name)
        }) else {
            continue;
        };
        let inverse = transform.to_matrix().inverse();
        let target = wanted
            .and_then(|(_, target)| *target)
            .map(|target| inverse.transform_point3(target).to_array());
        let mut result = Ok(None);
        let bones = xmodel_runtime::pose_dobj_with_controller(
            &slot.dobj,
            &request,
            Mat4::IDENTITY,
            |dobj, _, locals| {
                let animated = locals.to_vec();
                result = state.controller.advance(dobj, locals, target, at_ms);
                if result.is_err() {
                    locals.copy_from_slice(&animated);
                }
            },
        );
        let status = match (&result, &bones) {
            (_, Err(_)) => "animation_unavailable",
            (Err(xmodel_runtime::ActorGazeError::MissingHead), _) => "missing_head",
            (Err(xmodel_runtime::ActorGazeError::MissingNeck), _) => "missing_neck",
            (Err(xmodel_runtime::ActorGazeError::MissingEyeOrigin), _) => "missing_eye_origin",
            (Err(_), _) => "invalid_pose_or_target",
            (Ok(_), _) if wanted.is_some() && target.is_none() => "target_unavailable",
            (Ok(Some(_)), _) if target.is_some() => "active",
            (Ok(Some(_)), _) => "returning",
            (Ok(None), _) => "inactive",
        };
        if state.status != Some(status) {
            perf::instant!("actor_gaze_controller", "iw4l.render", "actor" => u32::from(number), "status" => status);
            state.status = Some(status);
        }
        state.pose = result.ok().flatten();
        state.bones = bones.ok().filter(|_| state.pose.is_some());
        if wanted.is_none() && !state.controller.active() {
            slot.gaze = None;
        }
    }
}

fn actor_eye(
    inputs: &Inputs<'_, '_>,
    owners: &Query<(Entity, &WorldScriptModelInstance, &Transform, &Visibility)>,
    persist: &mut ScriptModelDobjs,
    gaze: &sim::ActorGaze,
) -> Option<Vec3> {
    if !generation_matches(
        inputs.presented.snapshot()?,
        gaze.target_number,
        gaze.target_generation,
    ) {
        return None;
    }
    let (_, owner, transform, _) = owners
        .iter()
        .find(|(_, owner, _, _)| owner.gentity_number == Some(gaze.target_number))?;
    let id = owner.authority_owner?.script_model()?.to_wire();
    compose_or_reuse_script_dobj(
        persist,
        id,
        inputs.catalog.as_deref()?,
        &owner.dobj_state,
        &inputs.prepared,
    )?;
    let slot = persist.by_id.get(&id)?;
    let request = owner
        .dobj_state
        .resolve_request(|name| {
            inputs
                .xanims
                .as_ref()?
                .0
                .clip(asset_core::FamilyId::IW4, name)
        })
        .ok()?;
    let bones = xmodel_runtime::pose_dobj_with_controller(
        &slot.dobj,
        &request,
        Mat4::IDENTITY,
        |dobj, _, locals| {
            if let Some(state) = &slot.gaze {
                let animated = locals.to_vec();
                if state.controller.apply_current(dobj, locals).is_err() {
                    locals.copy_from_slice(&animated);
                }
            }
        },
    )
    .ok()?;
    Some(
        transform
            .to_matrix()
            .transform_point3(bones.get(slot.dobj.find("tag_eye")?)?.w_axis.truncate()),
    )
}
