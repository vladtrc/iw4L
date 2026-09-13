use std::collections::BTreeMap;
use std::path::Path;

use assets::{Iw5ZoneMemory, T5ZoneMemory, ZoneGame, ZoneMemory, games_root_from_env, open_zone};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let games = games_root_from_env()?;
    let zones: Vec<String> = std::env::args().skip(1).collect();
    if zones.is_empty() {
        eprintln!("usage: zonewalk <zone-or-path> [more...]");
        std::process::exit(2);
    }

    let mut all_clean = true;
    for name in &zones {
        match walk(&games, name) {
            Ok(clean) => all_clean &= clean,
            Err(err) => {
                println!("=== {name} ===");
                println!("  GATE           FAIL {err}");
                all_clean = false;
            }
        }
    }
    if !all_clean {
        std::process::exit(1);
    }
    Ok(())
}

fn resolve_zone_path(
    games: &assets::GamesRoot,
    name: &str,
) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let as_path = Path::new(name);
    if as_path.is_file() {
        return Ok(as_path.to_path_buf());
    }
    match assets::find_zone_file(games, name) {
        Ok(found) => Ok(found.path),
        Err(iw4_err) => {
            let t5 = games
                .0
                .join("pluto_t5_full_game/zone/Common")
                .join(format!("{name}.ff"));
            if t5.is_file() {
                Ok(t5)
            } else {
                Err(format!("{iw4_err} (also no T5 Common/{name}.ff)").into())
            }
        }
    }
}

fn walk(games: &assets::GamesRoot, name: &str) -> Result<bool, Box<dyn std::error::Error>> {
    let path = resolve_zone_path(games, name)?;
    let image = open_zone(&path)?;
    println!("=== {name} ===");
    println!("  game           {:?}", image.game);
    match image.game {
        ZoneGame::Iw4 => walk_iw4(&image),
        ZoneGame::T5 => walk_t5(&image),
        ZoneGame::Iw5 => walk_iw5(&image),
    }
}

fn walk_iw4(image: &assets::ZoneImage) -> Result<bool, Box<dyn std::error::Error>> {
    use fastfile_iw4::{
        AssetSink, AssetType, Ptr, ScriptStrings, ZoneStream, load_asset_at, load_zone,
    };

    #[derive(Default)]
    struct WalkSink {
        walked: usize,
        per_type: BTreeMap<&'static str, usize>,
        stopped_at: Option<(usize, &'static str)>,
        drift_by_type: BTreeMap<&'static str, usize>,
        first_drift_asset: Option<(usize, &'static str)>,
    }

    impl AssetSink for WalkSink {
        fn set_script_strings(&mut self, _strings: ScriptStrings) {}

        fn load_asset(
            &mut self,
            s: &mut ZoneStream<'_>,
            index: usize,
            ty: AssetType,
            slot: Ptr,
        ) -> fastfile_iw4::Result<()> {
            self.stopped_at = Some((index, ty.name()));
            if ty == AssetType::GfxWorld {
                println!(
                    "  before gfxworld  index={index} runtime_wm={} virt_wm={} peek={:02x?}",
                    s.watermark(2),
                    s.watermark(3),
                    s.peek(32)
                );
            }
            let drift_before = s.unsettled_offsets();
            load_asset_at(s, ty, slot)?;
            self.walked += 1;
            let drift = s.unsettled_offsets() - drift_before;
            if drift > 0 {
                *self.drift_by_type.entry(ty.name()).or_default() += drift;
                self.first_drift_asset.get_or_insert((index, ty.name()));
            }
            *self.per_type.entry(ty.name()).or_default() += 1;
            self.stopped_at = None;
            Ok(())
        }
    }

    let header = image.header()?;
    let mut memory = ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = WalkSink::default();
    let outcome = load_zone(&mut stream, &mut sink);
    let total = sink.walked + usize::from(sink.stopped_at.is_some());

    println!("  walked         {} assets", sink.walked);
    for (kind, count) in &sink.per_type {
        println!("  {count:>7}  {kind}");
    }
    println!(
        "  consumed       {} of {} payload bytes ({} remaining)",
        stream.cursor(),
        stream.len(),
        stream.remaining()
    );
    println!("  unsettled      {}", stream.unsettled_offsets());
    if let Some((p, watermark, cursor)) = stream.first_unsettled() {
        println!(
            "  first drift    block {} offset {} (block filled to {}, stream cursor {})",
            p.block, p.offset, watermark, cursor
        );
    }
    if !sink.drift_by_type.is_empty() {
        println!("  drift by type  {:?}", sink.drift_by_type);
        println!("  first drift in {:?}", sink.first_drift_asset);
    }
    if let Some(g) = stream.gfx_world() {
        println!(
            "  gfxworld       {} vertices, {} indices, {} surfaces",
            g.vertex_count, g.index_count, g.surface_count
        );
    }
    if let Some(g) = stream.fx_world() {
        println!(
            "  fxworld glass  init_pieces={} defs={} init_geo={} piece_limit={}",
            g.init_piece_count, g.def_count, g.init_geo_count, g.piece_limit
        );
    } else {
        println!("  fxworld glass  (not recorded)");
    }
    if let Some(g) = stream.g_glass_data() {
        println!(
            "  g_glassData    pieces={} names={}",
            g.piece_count, g.name_count
        );
    } else {
        println!("  g_glassData    (not recorded)");
    }

    match outcome {
        Ok(_) => match stream.finish() {
            Ok(()) if stream.unsettled_offsets() == 0 => {
                println!("  GATE           PASS");
                Ok(true)
            }
            Ok(()) => {
                println!("  GATE           FAIL — offsets drifted");
                Ok(false)
            }
            Err(e) => {
                println!("  GATE           FAIL — {e}");
                Ok(false)
            }
        },
        Err(e) => {
            let at = sink
                .stopped_at
                .map(|(i, t)| format!("asset {i} ({t})"))
                .unwrap_or_else(|| format!("after asset {total}"));
            let (tag, align, bytes) = stream.last_alloc_tag();
            println!("  GATE           FAIL — stopped at {at}: {e}");
            println!(
                "  last alloc     tag=+{tag} align={align} bytes={bytes} runtime_wm={} virt_wm={}",
                stream.watermark(2),
                stream.watermark(3)
            );
            println!("  next byte      {:02x?}", stream.peek(16));
            Ok(false)
        }
    }
}

fn walk_t5(image: &assets::ZoneImage) -> Result<bool, Box<dyn std::error::Error>> {
    use fastfile_t5::{
        AssetSink, AssetType, Ptr, ScriptStrings, ZoneStream, load_asset_at, load_zone,
    };

    #[derive(Default)]
    struct WalkSink {
        walked: usize,
        per_type: BTreeMap<&'static str, usize>,
        stopped_at: Option<(usize, &'static str)>,
        drift_by_type: BTreeMap<&'static str, usize>,
        first_drift_asset: Option<(usize, &'static str)>,
    }

    impl AssetSink for WalkSink {
        fn set_script_strings(&mut self, _strings: ScriptStrings) {}

        fn load_asset(
            &mut self,
            s: &mut ZoneStream<'_>,
            index: usize,
            ty: AssetType,
            slot: Ptr,
        ) -> fastfile_t5::Result<()> {
            self.stopped_at = Some((index, ty.name()));
            let drift_before = s.unsettled_offsets();
            load_asset_at(s, ty, slot)?;
            self.walked += 1;
            let drift = s.unsettled_offsets() - drift_before;
            if drift > 0 {
                *self.drift_by_type.entry(ty.name()).or_default() += drift;
                self.first_drift_asset.get_or_insert((index, ty.name()));
            }
            *self.per_type.entry(ty.name()).or_default() += 1;
            self.stopped_at = None;
            Ok(())
        }
    }

    let header = image.t5_header()?;
    let mut memory = T5ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = WalkSink::default();
    let outcome = load_zone(&mut stream, &mut sink);
    let total = sink.walked + usize::from(sink.stopped_at.is_some());

    println!("  walked         {} assets", sink.walked);
    for (kind, count) in &sink.per_type {
        println!("  {count:>7}  {kind}");
    }
    println!(
        "  consumed       {} of {} payload bytes ({} remaining)",
        stream.cursor(),
        stream.len(),
        stream.remaining()
    );
    println!("  unsettled      {}", stream.unsettled_offsets());
    if let Some((p, watermark, cursor)) = stream.first_unsettled() {
        println!(
            "  first drift    block {} offset {} (block filled to {}, stream cursor {})",
            p.block, p.offset, watermark, cursor
        );
    }
    if !sink.drift_by_type.is_empty() {
        println!("  drift by type  {:?}", sink.drift_by_type);
        println!("  first drift in {:?}", sink.first_drift_asset);
    }

    match outcome {
        Ok(_) => match stream.finish() {
            Ok(()) if stream.unsettled_offsets() == 0 => {
                println!("  GATE           PASS");
                Ok(true)
            }
            Ok(()) => {
                println!("  GATE           FAIL — offsets drifted");
                Ok(false)
            }
            Err(e) => {
                println!("  GATE           FAIL — {e}");
                Ok(false)
            }
        },
        Err(e) => {
            let at = sink
                .stopped_at
                .map(|(i, t)| format!("asset {i} ({t})"))
                .unwrap_or_else(|| format!("after asset {total}"));
            println!("  GATE           FAIL — stopped at {at}: {e}");
            println!("  next byte      {:02x?}", stream.peek(16));
            Ok(false)
        }
    }
}

fn walk_iw5(image: &assets::ZoneImage) -> Result<bool, Box<dyn std::error::Error>> {
    use fastfile_iw5::{
        AssetSink, AssetType, Ptr, ScriptStrings, ZoneStream, load_asset_at, load_zone,
    };

    #[derive(Default)]
    struct WalkSink {
        walked: usize,
        per_type: BTreeMap<&'static str, usize>,
        stopped_at: Option<(usize, &'static str)>,
    }

    impl AssetSink for WalkSink {
        fn set_script_strings(&mut self, _strings: ScriptStrings) {}

        fn load_asset(
            &mut self,
            s: &mut ZoneStream<'_>,
            index: usize,
            ty: AssetType,
            slot: Ptr,
        ) -> fastfile_iw5::Result<()> {
            self.stopped_at = Some((index, ty.name()));
            load_asset_at(s, ty, slot)?;
            self.walked += 1;
            *self.per_type.entry(ty.name()).or_default() += 1;
            self.stopped_at = None;
            Ok(())
        }
    }

    let header = image.iw5_header()?;
    let mut memory = Iw5ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = WalkSink::default();
    let outcome = load_zone(&mut stream, &mut sink);
    let total = sink.walked + usize::from(sink.stopped_at.is_some());

    println!("  walked         {} assets", sink.walked);
    for (kind, count) in &sink.per_type {
        println!("  {count:>7}  {kind}");
    }
    println!(
        "  consumed       {} of {} payload bytes ({} remaining)",
        stream.cursor(),
        stream.len(),
        stream.remaining()
    );
    println!("  unsettled      {}", stream.unsettled_offsets());

    match outcome {
        Ok(_) => match stream.finish() {
            Ok(()) if stream.unsettled_offsets() == 0 => {
                println!("  GATE           PASS");
                Ok(true)
            }
            Ok(()) => {
                println!("  GATE           FAIL — offsets drifted");
                Ok(false)
            }
            Err(e) => {
                println!("  GATE           FAIL — {e}");
                Ok(false)
            }
        },
        Err(e) => {
            let at = sink
                .stopped_at
                .map(|(i, t)| format!("asset {i} ({t})"))
                .unwrap_or_else(|| format!("after asset {total}"));
            println!("  GATE           FAIL — stopped at {at}: {e}");
            println!("  next byte      {:02x?}", stream.peek(16));
            Ok(false)
        }
    }
}
