use super::snd::follow_snd_alias_custom;
use super::{AssetLinkSink, asset_ptr_at, follow_name};
use crate::asset_type::AssetType;
use crate::size as sz;
use crate::zone::{Result, XFILE_BLOCK_VIRTUAL, ZoneStream};

pub(super) fn load_vehicle(s: &mut ZoneStream<'_>, links: &mut dyn AssetLinkSink) -> Result<()> {
    let width = s.pointer_bytes();
    let p = s.alloc_load(4, s.layout(sz::VEHICLE_DEF, 1336))?;
    s.push(XFILE_BLOCK_VIRTUAL)?;
    follow_name(s, p, 0)?;
    follow_name(s, p, s.layout(8, 16))?;
    let phys = p.at(s.layout(172, 184));
    follow_name(s, phys, s.layout(4, 8))?;
    asset_ptr_at(s, links, AssetType::PhysPreset, phys.at(s.layout(8, 16)))?;
    follow_name(s, phys, s.layout(12, 24))?;
    follow_name(s, p, s.layout(640, 672))?;
    asset_ptr_at(s, links, AssetType::Weapon, p.at(s.layout(644, 680)))?;
    follow_snd_alias_custom(s, p.at(s.layout(684, 728)))?;
    follow_snd_alias_custom(s, p.at(s.layout(688, 736)))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(720, 776)))?;
    asset_ptr_at(s, links, AssetType::Fx, p.at(s.layout(724, 784)))?;
    for off in [
        s.layout(728, 792),
        s.layout(732, 800),
        s.layout(736, 808),
        s.layout(740, 816),
    ] {
        asset_ptr_at(s, links, AssetType::Material, p.at(off))?;
    }
    for off in [
        s.layout(752, 832),
        s.layout(756, 840),
        s.layout(760, 848),
        s.layout(764, 856),
        s.layout(776, 872),
        s.layout(780, 880),
        s.layout(784, 888),
        s.layout(788, 896),
        s.layout(800, 912),
        s.layout(804, 920),
        s.layout(808, 928),
        s.layout(816, 944),
        s.layout(820, 952),
        s.layout(824, 960),
        s.layout(828, 968),
        s.layout(836, 984),
        s.layout(844, 1000),
        s.layout(852, 1016),
        s.layout(860, 1032),
        s.layout(868, 1048),
    ] {
        follow_snd_alias_custom(s, p.at(off))?;
    }
    follow_name(s, p, s.layout(876, 1064))?;
    let surface_snds = s.layout(880, 1072);
    for i in 0..sz::VEHICLE_SURFACE_SND_COUNT {
        follow_snd_alias_custom(s, p.at(surface_snds + i * width))?;
    }
    s.pop()
}
