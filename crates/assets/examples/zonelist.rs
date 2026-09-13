use std::collections::BTreeMap;
use std::path::Path;

use assets::{Iw5ZoneMemory, T5ZoneMemory, ZoneGame, ZoneMemory, games_root_from_env, open_zone};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let games = games_root_from_env()?;
    let zones: Vec<String> = std::env::args().skip(1).collect();
    if zones.is_empty() {
        eprintln!("usage: zonelist <zone-or-path> [more...]");
        std::process::exit(2);
    }

    let mut failures = 0usize;
    for name in &zones {
        if let Err(e) = report(&games, name) {
            eprintln!("=== {name} ===\n  FAILED: {e}");
            failures += 1;
        }
    }

    if failures != 0 {
        std::process::exit(1);
    }
    Ok(())
}

fn report(games: &assets::GamesRoot, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    let path = resolve_zone_path(games, name)?;
    let image = open_zone(&path)?;

    println!("=== {name} ===");
    println!("  file           {}", path.display());
    println!("  game           {:?}", image.game);
    println!("  version        {:#x}", image.version);
    println!("  inflated       {} bytes", image.bytes.len());

    match image.game {
        ZoneGame::Iw4 => report_iw4(&image)?,
        ZoneGame::T5 => report_t5(&image)?,
        ZoneGame::Iw5 => report_iw5(&image)?,
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

fn report_iw4(image: &assets::ZoneImage) -> Result<(), Box<dyn std::error::Error>> {
    let wire_table = image.iw4_table()?;
    println!("  wire format    {:?}", wire_table.format());
    let header = image.header()?;
    println!("  XFile.size     {}", header.size);
    println!("  block sizes    {:?}", header.block_size);

    let mut memory = ZoneMemory::for_header(&header);
    println!("  arenas         {} bytes", memory.total_bytes());

    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let table = fastfile_iw4::open_asset_table(&mut stream).map_err(|e| e.to_string())?;
    print_table_iw4(&stream, &table)?;
    Ok(())
}

fn report_iw5(image: &assets::ZoneImage) -> Result<(), Box<dyn std::error::Error>> {
    let header = image.iw5_header()?;
    println!("  XFile.size     {}", header.size);
    println!("  block sizes    {:?}", header.block_size);

    let mut memory = Iw5ZoneMemory::for_header(&header);
    println!("  arenas         {} bytes", memory.total_bytes());

    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let table = fastfile_iw5::open_asset_table(&mut stream).map_err(|e| e.to_string())?;
    print_table_iw5(&stream, &table)?;
    Ok(())
}

fn report_t5(image: &assets::ZoneImage) -> Result<(), Box<dyn std::error::Error>> {
    let header = image.t5_header()?;
    println!("  XFile.size     {}", header.size);
    println!("  block sizes    {:?}", header.block_size);

    let mut memory = T5ZoneMemory::for_header(&header);
    println!("  arenas         {} bytes", memory.total_bytes());

    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let table = fastfile_t5::open_asset_table(&mut stream).map_err(|e| e.to_string())?;
    print_table_t5(&stream, &table)?;
    Ok(())
}

fn print_table_iw4(
    stream: &fastfile_iw4::ZoneStream<'_>,
    table: &fastfile_iw4::AssetTable,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("  scriptStrings  {}", table.strings.count());
    let sample: Vec<&str> = (0..table.strings.count().min(5) as u16)
        .filter_map(|i| table.strings.get(stream, i))
        .collect();
    println!("  first strings  {sample:?}");
    println!("  assetCount     {}", table.count());

    let mut histogram: BTreeMap<&'static str, usize> = BTreeMap::new();
    for i in 0..table.count() {
        let kind = table.kind(stream, i).map_err(|e| e.to_string())?;
        *histogram.entry(kind.name()).or_default() += 1;
    }
    for (kind, count) in &histogram {
        println!("  {count:>7}  {kind}");
    }

    let mut runs: Vec<(&'static str, usize, usize)> = Vec::new();
    for i in 0..table.count() {
        let kind = table.kind(stream, i).map_err(|e| e.to_string())?.name();
        match runs.last_mut() {
            Some(last) if last.0 == kind => last.2 += 1,
            _ => runs.push((kind, i, 1)),
        }
    }
    println!("  load order     {} runs", runs.len());
    for (kind, first, len) in &runs {
        println!("  [{first:>4}..{:<4}] {len:>4}  {kind}", first + len);
    }

    println!(
        "  consumed       {} of {} payload bytes ({} remaining — asset bodies not loaded yet)",
        stream.cursor(),
        stream.len(),
        stream.remaining()
    );
    println!("  unsettled      {}", stream.unsettled_offsets());
    Ok(())
}

fn print_table_t5(
    stream: &fastfile_t5::ZoneStream<'_>,
    table: &fastfile_t5::AssetTable,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("  scriptStrings  {}", table.strings.count());
    let sample: Vec<&str> = (0..table.strings.count().min(5) as u16)
        .filter_map(|i| table.strings.get(stream, i))
        .collect();
    println!("  first strings  {sample:?}");
    println!("  assetCount     {}", table.count());

    let mut histogram: BTreeMap<&'static str, usize> = BTreeMap::new();
    for i in 0..table.count() {
        let kind = table.kind(stream, i).map_err(|e| e.to_string())?;
        *histogram.entry(kind.name()).or_default() += 1;
    }
    for (kind, count) in &histogram {
        println!("  {count:>7}  {kind}");
    }

    let mut runs: Vec<(&'static str, usize, usize)> = Vec::new();
    for i in 0..table.count() {
        let kind = table.kind(stream, i).map_err(|e| e.to_string())?.name();
        match runs.last_mut() {
            Some(last) if last.0 == kind => last.2 += 1,
            _ => runs.push((kind, i, 1)),
        }
    }
    println!("  load order     {} runs", runs.len());
    for (kind, first, len) in &runs {
        println!("  [{first:>4}..{:<4}] {len:>4}  {kind}", first + len);
    }

    println!(
        "  consumed       {} of {} payload bytes ({} remaining — asset bodies not loaded yet)",
        stream.cursor(),
        stream.len(),
        stream.remaining()
    );
    println!("  unsettled      {}", stream.unsettled_offsets());
    Ok(())
}

fn print_table_iw5(
    stream: &fastfile_iw5::ZoneStream<'_>,
    table: &fastfile_iw5::AssetTable,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("  scriptStrings  {}", table.strings.count());
    let sample: Vec<&str> = (0..table.strings.count().min(5) as u16)
        .filter_map(|i| table.strings.get(stream, i))
        .collect();
    println!("  first strings  {sample:?}");
    println!("  assetCount     {}", table.count());

    let mut histogram: BTreeMap<&'static str, usize> = BTreeMap::new();
    for i in 0..table.count() {
        let kind = table.kind(stream, i).map_err(|e| e.to_string())?;
        *histogram.entry(kind.name()).or_default() += 1;
    }
    for (kind, count) in &histogram {
        println!("  {count:>7}  {kind}");
    }
    println!(
        "  consumed       {} of {} payload bytes ({} remaining — asset bodies not loaded yet)",
        stream.cursor(),
        stream.len(),
        stream.remaining()
    );
    println!("  unsettled      {}", stream.unsettled_offsets());
    Ok(())
}
