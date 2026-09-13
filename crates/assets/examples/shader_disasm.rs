use assets::{
    Iw5ZoneMemory, MaterialCatalog, ZoneGame, ZoneMemory, games_root_from_env, open_zone,
};
use fastfile_iw4::shader::Shader;

fn print_shaders(catalog: &MaterialCatalog, wanted: &[String]) -> usize {
    let mut hits = 0usize;
    for shader in &catalog.shaders {
        if !wanted
            .iter()
            .any(|want| shader.name.as_str().contains(want.as_str()))
        {
            continue;
        }
        hits += 1;
        let texldd = shader_opcode_count(&shader.program, 0x5d);
        let texldl = shader_opcode_count(&shader.program, 0x5f);
        let texld = shader_opcode_count(&shader.program, 0x42);
        println!(
            "--- {} ({} bytes) pixel={} texld={texld} texldl={texldl} texldd={texldd} ---",
            shader.name,
            shader.program.len(),
            shader.is_pixel(),
        );
        match Shader::parse(&shader.program) {
            Ok(parsed) => {
                if let Ok(Some(ctab)) = parsed.ctab() {
                    for index in 0..ctab.len() {
                        let Ok(constant) = ctab.constant(index) else {
                            continue;
                        };
                        println!(
                            "    const {:<36} set={} reg={}",
                            String::from_utf8_lossy(constant.name),
                            constant.register_set,
                            constant.register_index
                        );
                    }
                }
                println!("{}", parsed.disassembly());
            }
            Err(error) => println!("    disassembly failed: {error:?}"),
        }
    }
    hits
}

fn shader_opcode_count(bytes: &[u8], opcode: u16) -> usize {
    bytes
        .chunks_exact(4)
        .filter(|word| u16::from_le_bytes([word[0], word[1]]) == opcode)
        .count()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let games = games_root_from_env()?;
    let mut args = std::env::args().skip(1);
    let Some(zone) = args.next() else {
        eprintln!("usage: shader_disasm <zone> <name-substring>...");
        std::process::exit(2);
    };
    let wanted: Vec<String> = args.collect();
    if wanted.is_empty() {
        eprintln!("usage: shader_disasm <zone> <name-substring>...");
        std::process::exit(2);
    }

    let found = assets::find_zone_file(&games, &zone)?;
    let image = open_zone(&found.path)?;
    println!("=== {zone} game={:?} ===", image.game);

    let catalog = match image.game {
        ZoneGame::Iw4 => load_iw4(&image)?,
        ZoneGame::Iw5 => load_iw5(&image)?,
        ZoneGame::T5 => {
            eprintln!("shader_disasm: T5 zones not wired (use a T5 example)");
            std::process::exit(2);
        }
    };
    println!("shaders {}", catalog.shaders.len());

    let hits = print_shaders(&catalog, &wanted);
    if hits == 0 {
        println!("no shader name matched {wanted:?}");
        std::process::exit(1);
    }
    Ok(())
}

fn load_iw4(image: &assets::ZoneImage) -> Result<MaterialCatalog, Box<dyn std::error::Error>> {
    use fastfile_iw4::{
        AssetLinkSink, AssetSink, AssetType, Ptr, ScriptStrings, ZoneStream,
        load_asset_at_observed, load_zone,
    };

    #[derive(Default)]
    struct ProbeSink {
        materials: MaterialCatalog,
    }
    impl AssetSink for ProbeSink {
        fn set_script_strings(&mut self, _strings: ScriptStrings) {}
        fn load_asset(
            &mut self,
            s: &mut ZoneStream<'_>,
            _index: usize,
            ty: AssetType,
            slot: Ptr,
        ) -> fastfile_iw4::Result<()> {
            load_asset_at_observed(s, ty, slot, self)?;
            Ok(())
        }
    }
    impl AssetLinkSink for ProbeSink {
        fn loaded(
            &mut self,
            stream: &ZoneStream<'_>,
            ty: AssetType,
            slot: Ptr,
            insert_slot: Option<Ptr>,
        ) -> fastfile_iw4::Result<()> {
            self.materials.loaded(stream, ty, slot, insert_slot)
        }
        fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> fastfile_iw4::Result<()> {
            self.materials.alias(ty, slot, target)
        }
    }

    let header = image.header()?;
    let mut memory = ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = ProbeSink::default();
    let _ = load_zone(&mut stream, &mut sink);
    Ok(sink.materials)
}

fn load_iw5(image: &assets::ZoneImage) -> Result<MaterialCatalog, Box<dyn std::error::Error>> {
    use fastfile_iw5::{
        AssetLinkSink, AssetSink, AssetType, Ptr, ScriptStrings, ZoneStream,
        load_asset_at_observed, load_zone,
    };

    #[derive(Default)]
    struct ProbeSink {
        materials: MaterialCatalog,
    }
    impl AssetSink for ProbeSink {
        fn set_script_strings(&mut self, _strings: ScriptStrings) {}
        fn load_asset(
            &mut self,
            s: &mut ZoneStream<'_>,
            _index: usize,
            ty: AssetType,
            slot: Ptr,
        ) -> fastfile_iw5::Result<()> {
            load_asset_at_observed(s, ty, slot, self)?;
            Ok(())
        }
    }
    impl AssetLinkSink for ProbeSink {
        fn loaded(
            &mut self,
            stream: &ZoneStream<'_>,
            ty: AssetType,
            slot: Ptr,
            insert_slot: Option<Ptr>,
        ) -> fastfile_iw5::Result<()> {
            self.materials.iw5_loaded(stream, ty, slot, insert_slot);
            Ok(())
        }
        fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> fastfile_iw5::Result<()> {
            self.materials.iw5_alias(ty, slot, target);
            Ok(())
        }
    }

    let header = image.iw5_header()?;
    let mut memory = Iw5ZoneMemory::for_header(&header);
    let mut stream = memory.stream(&image.bytes).map_err(|e| e.to_string())?;
    let mut sink = ProbeSink::default();
    let _ = load_zone(&mut stream, &mut sink);
    Ok(sink.materials)
}
