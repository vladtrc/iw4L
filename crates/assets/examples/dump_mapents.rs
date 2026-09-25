use fastfile_iw4::{AssetLinkSink, AssetSink, AssetType, Ptr, ScriptStrings, ZoneStream, load_asset_at_observed, load_zone};
struct Null;
impl AssetSink for Null {
    fn set_script_strings(&mut self, _: ScriptStrings) {}
    fn load_asset(&mut self, s: &mut ZoneStream<'_>, _: usize, ty: AssetType, slot: Ptr) -> fastfile_iw4::Result<()> {
        load_asset_at_observed(s, ty, slot, self).map(|_| ())
    }
}
impl AssetLinkSink for Null {
    fn loaded(&mut self, _: &ZoneStream<'_>, _: AssetType, _: Ptr, _: Option<Ptr>) -> fastfile_iw4::Result<()> { Ok(()) }
    fn alias(&mut self, _: AssetType, _: Ptr, _: Ptr) -> fastfile_iw4::Result<()> { Ok(()) }
}
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let image = asset_transport::zone::open_zone_shared(&path).unwrap();
    let header = image.header().unwrap();
    let mut memory = asset_transport::zone::ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).unwrap();
    load_zone(&mut stream, &mut Null).unwrap();
    print!("{}", asset_world::map_ents_entity_string(&stream).unwrap());
}
