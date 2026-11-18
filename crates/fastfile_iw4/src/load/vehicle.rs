use asset_iw4::size as sz;

use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::zone::{Ptr, Result, XFILE_BLOCK_VIRTUAL, ZonePtr, ZoneStream};

pub(super) fn load_vehicle(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let p = s.alloc_load(4, s.layout(sz::VEHICLE_DEF, 1008))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    let name = s.follow_string(p, 0)?;
    s.follow_string(p, s.layout(8, 16))?;

    s.follow_string(p, s.layout(172, 192))?;
    asset_ptr_at(s, links, AssetType::PhysPreset, p.at(s.layout(176, 200)))?;
    s.follow_string(p, s.layout(180, 208))?;

    let turret_weapon = s.follow_string(p, s.layout(408, 448))?;
    let accel = s.f32_at(p, s.layout(28, 40))?;
    let kind = s.u32_at(p, s.layout(4, 8))?;
    let physics = s.i32_at(p, s.layout(168, 184))? != 0;
    let suspension = [
        s.f32_at(p, s.layout(216, 248))?,
        s.f32_at(p, s.layout(220, 252))?,
    ];
    s.record_vehicle_motion(accel, kind, physics, suspension);
    s.record_vehicle(crate::zone::VehicleGeometry {
        header: p,
        name,
        turret_weapon,
    });
    asset_ptr_at(s, links, AssetType::Weapon, p.at(s.layout(412, 456)))?;

    follow_snd_alias_custom(s, p.at(s.layout(436, 488)))?;
    follow_snd_alias_custom(s, p.at(s.layout(440, 496)))?;
    let mut icons = [[0; 128]; 2];
    for (index, (field, wide)) in [(472, 536), (476, 544)].into_iter().enumerate() {
        let slot = p.at(s.layout(field, wide));
        let fresh = super::asset_ptr_at_linked(s, links, AssetType::Material, slot)?;
        super::copy_linked_material(s, links, slot, fresh, &mut icons[index]);
    }
    s.record_vehicle_compass(
        icons,
        [
            s.i32_at(p, s.layout(480, 552))?,
            s.i32_at(p, s.layout(484, 556))?,
        ],
    );
    for (field, wide) in [
        (488, 560),
        (492, 568),
        (496, 576),
        (500, 584),
        (508, 600),
        (516, 616),
        (520, 624),
        (524, 632),
        (528, 640),
        (536, 656),
        (544, 672),
        (552, 688),
        (560, 704),
        (568, 720),
    ] {
        follow_snd_alias_custom(s, p.at(s.layout(field, wide)))?;
    }

    let mut engine_names = [None; 10];
    for (index, (field, wide)) in [
        (488, 560),
        (492, 568),
        (496, 576),
        (500, 584),
        (508, 600),
        (516, 616),
        (520, 624),
        (524, 632),
        (528, 640),
        (536, 656),
    ]
    .into_iter()
    .enumerate()
    {
        if let ZonePtr::Offset(custom) = s.ptr_at(p, s.layout(field, wide))? {
            let custom = s.resolve_alias(custom);
            if let ZonePtr::Offset(name) = s.ptr_at(custom, 0)? {
                engine_names[index] = Some(s.resolve_alias(name));
            }
        }
    }
    let durations = [
        s.i32_at(p, s.layout(512, 608))?,
        s.i32_at(p, s.layout(532, 648))?,
        s.i32_at(p, s.layout(540, 664))?,
    ];
    s.record_vehicle_engine(engine_names, durations, s.f32_at(p, s.layout(504, 592))?);

    s.follow_string(p, s.layout(576, 736))?;
    for i in 0..sz::SURF_TYPE_NUM {
        follow_snd_alias_custom(s, p.at(s.layout(580, 744) + i * s.pointer_bytes()))?;
    }

    s.pop()
}

fn follow_snd_alias_custom(s: &mut ZoneStream<'_>, slot: Ptr) -> Result<()> {
    match s.ptr_at(slot, 0)? {
        ZonePtr::Null => Ok(()),
        ZonePtr::Offset(q) => {
            s.note_offset(q);
            Ok(())
        }
        _ => {
            if !s.begin_body(slot)? {
                return Ok(());
            }
            let n = s.alloc_load(4, s.layout(sz::SND_ALIAS_CUSTOM, 8))?;
            follow_name(s, n, 0)?;
            s.fixup_slot(slot, n)
        }
    }
}
