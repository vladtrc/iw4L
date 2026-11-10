use std::path::Path;

const USAGE: &str = "usage: iw4l-content solve-shot [--replace-victim] <route.json>\n       iw4l-content connect-maps [--seed N] <zone>...\n       iw4l-content inspect-doors <zone>...\n       iw4l-content inspect-layout <zone>...\n       iw4l-content inspect-projectile-openings <zone>...\n       iw4l-content recipe-build <scene.yml> <scene.json>\n       iw4l-content weapon-bindings <zone> <output.json>\n       iw4l-content trace-map <zone> <queries.yml> <output.json>\n       iw4l-content probe-floor <zone> <points.json>\n       iw4l-content probe-sweep <zone> <queries.json>\n       iw4l-content validate <scene.json>\n       iw4l-content migrate <old.json> <new.json>";

fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or(USAGE)?;
    if command == "--help" || command == "-h" {
        println!("{USAGE}");
        return Ok(());
    }
    game_family::install(&[
        &game_family::Iw4Family,
        #[cfg(feature = "t5")]
        &ext_t5::Family,
        #[cfg(feature = "t6")]
        &ext_t6::Family,
        #[cfg(feature = "iw5")]
        &ext_iw5::Family,
    ])
    .map_err(str::to_owned)?;
    asset_transport::install_games_provider(desktop::DesktopGamesProvider)
        .map_err(str::to_owned)?;
    let artifacts = asset_transport::ensure_artifacts_dir()?;
    let rest: Vec<String> = args.collect();
    if command == "migrate" && rest.len() == 2 {
        return content_gen::migrate(Path::new(&rest[0]), Path::new(&rest[1]));
    }
    if command == "recipe-build" && rest.len() == 2 {
        return content_gen::compile_recipe(Path::new(&rest[0]), Path::new(&rest[1]));
    }
    let games = asset_transport::games_root()?;
    desktop::link_steam_games(&games);
    match command.as_str() {
        "trace-map" if rest.len() == 3 => {
            content_gen::trace_map(&games, &rest[0], Path::new(&rest[1]), Path::new(&rest[2]))
        }
        "weapon-bindings" if rest.len() == 2 => {
            content_gen::weapon_bindings(&games, &rest[0], Path::new(&rest[1]))
        }
        "solve-shot" => {
            let (replace, path) = match rest.as_slice() {
                [path] => (false, path),
                [flag, path] if flag == "--replace-victim" => (true, path),
                _ => return Err(USAGE.into()),
            };
            content_gen::shot_solver::solve(&games, &artifacts, Path::new(path), replace)
        }
        "connect-maps" if !rest.is_empty() => {
            let (seed, maps) = if rest.first().is_some_and(|v| v == "--seed") {
                let seed = rest
                    .get(1)
                    .ok_or("--seed needs a number")?
                    .parse::<u64>()
                    .map_err(|e| e.to_string())?;
                (Some(seed), &rest[2..])
            } else {
                (None, rest.as_slice())
            };
            content_gen::random_door::connect_maps(&games, &artifacts, maps, seed)
        }
        "inspect-projectile-openings" if !rest.is_empty() => {
            content_gen::doors::run_projectile_openings(games, artifacts, rest)
        }
        "inspect-doors" if !rest.is_empty() => {
            content_gen::doors::run_doors(games, artifacts, rest)
        }
        "inspect-layout" if !rest.is_empty() => {
            for zone in rest {
                content_gen::authoring::inspect_layout(&games, &artifacts, &zone)?;
            }
            Ok(())
        }
        "probe-floor" if rest.len() == 2 => {
            content_gen::authoring::probe_floor(&games, &artifacts, &rest[0], Path::new(&rest[1]))
        }
        "probe-sweep" if rest.len() == 2 => {
            content_gen::authoring::probe_sweep(&games, &artifacts, &rest[0], Path::new(&rest[1]))
        }
        "validate" if rest.len() == 1 => content_gen::validate(&games, Path::new(&rest[0])),
        _ => Err(USAGE.into()),
    }
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
