use super::args::string;
use super::natives::engine::entity_id;
use crate::bullet_collision::{AuthorityDObjState, PartSelection};
use crate::frame::FrameWorld;
use crate::script::{Namespace, NativeRegistry, RoundScript, Value};
use bevy_ecs::prelude::World;

fn edit(
    world: &mut World,
    receiver: &Value,
    change: impl FnOnce(&mut AuthorityDObjState) -> Result<(), String>,
) -> Result<Value, String> {
    let object = entity_id(world, receiver)?;
    if !world.resource::<RoundScript>().can_receive_call(&object) {
        return Err("entity identity is no longer live".into());
    }
    let apply = |dobj: &mut AuthorityDObjState| {
        if !dobj.has_part_model() {
            return Err("entity has no model".into());
        }
        change(dobj)
    };
    if world
        .resource::<RoundScript>()
        .player_client(object)
        .is_some()
    {
        super::model_state::player_appearance::edit_model(world, object, apply)?;
    } else if let super::entities::EntityKind::Item(number) =
        world.resource::<RoundScript>().entities[&object].kind
    {
        let mut frame = FrameWorld::from_world(world);
        let item = frame
            .dropped_item_by_number(number)
            .ok_or("item identity is no longer live")?;
        let weapon = u32::try_from(item.state.index).map_err(|_| "entity has no model")?;
        let mut dobj = crate::item::model::bind(&frame, weapon, item.state.weapon_model)?;
        dobj.set_part_mask(xmodel_runtime::HidePartBits::from_words(
            item.hide_part_bits,
        ));
        apply(&mut dobj)?;
        frame
            .dropped_item_mut_by_number(number)
            .ok_or("item identity is no longer live")?
            .hide_part_bits = *dobj.semantic_state.hide_part_bits.words();
    } else {
        super::presence::settle_collision(world);
        let presence = world.resource::<RoundScript>().entities[&object]
            .presence
            .ok_or("entity has no model")?;
        let mut frame = FrameWorld::from_world(world);
        let dobj = frame
            .collision_owner_mut(presence)
            .and_then(|row| row.dobj.as_mut())
            .ok_or("entity has no model")?;
        apply(dobj)?;
    }
    Ok(Value::Undefined)
}

fn named(
    world: &mut World,
    receiver: &Value,
    args: &[Value],
    hidden: bool,
    all: bool,
) -> Result<Value, String> {
    edit(world, receiver, |dobj| {
        let tag = string(args, 0)?.to_ascii_lowercase();
        let filtered = if all { args.len() == 2 } else { args.len() > 1 };
        let model = if filtered {
            Some(string(args, 1)?)
        } else {
            None
        };
        let selection = if all {
            PartSelection::Instances {
                tag: &tag,
                model: model.as_deref(),
            }
        } else {
            PartSelection::One {
                tag: &tag,
                model: model.as_deref(),
            }
        };
        dobj.set_parts_hidden(selection, hidden)
    })
}

pub(crate) fn register(registry: &mut NativeRegistry) {
    registry.register(Namespace::Method, "hidepart", |w, r, a| {
        named(w, r, a, true, false)
    });
    registry.register(Namespace::Method, "showpart", |w, r, a| {
        named(w, r, a, false, false)
    });
    registry.register(Namespace::Method, "hidepart_allinstances", |w, r, a| {
        named(w, r, a, true, true)
    });
    registry.register(Namespace::Method, "hideallparts", |w, r, _| {
        edit(w, r, |d| d.set_parts_hidden(PartSelection::All, true))
    });
    registry.register(Namespace::Method, "showallparts", |w, r, _| {
        edit(w, r, |d| d.set_parts_hidden(PartSelection::All, false))
    });
}
