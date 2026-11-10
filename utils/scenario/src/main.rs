use scenario::{
    Result,
    author::{Bindings, Program, Scene},
    read, write,
};
use serde_json::Value;
use std::path::Path;
const USAGE: &str = "iw4l-scenario build <scene.yml> <bindings.yml> <output-dir>\n  verify <build-dir> <receipts.json> <verdict.json>\n  compare <expected-take.json> <actual-take.json>\n  convert <input.yml|json> <output.json|yml>";
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        ["--help" | "-h"] => println!("{USAGE}"),
        ["build", scene_source, bindings_source, out] => {
            let scene: Scene = read(Path::new(scene_source))?;
            let bindings: Bindings = read(Path::new(bindings_source))?;
            let (p, m) = scene.compile(&bindings)?;
            write(&Path::new(out).join("take.json"), &p)?;
            write(&Path::new(out).join("manifest.json"), &m)?;
            scenario::record_sources(
                Path::new(out),
                &[Path::new(scene_source), Path::new(bindings_source)],
            )?;
            println!("Built {}", scene.name);
        }
        ["verify", build, receipts, out] => {
            let p = read(&Path::new(build).join("take.json"))?;
            let m = read(&Path::new(build).join("manifest.json"))?;
            let d = read(Path::new(receipts))?;
            let report = scenario::verify::declared(&p, &m, &d)?;
            write(Path::new(out), &report)?;
            if report["passed"] != true {
                return Err("take verification failed".into());
            }
            println!("Take verified");
        }
        ["compare", expected, actual] => {
            let p: Program = read(Path::new(expected))?;
            let q: Program = read(Path::new(actual))?;
            if serde_json::to_value(p)? != serde_json::to_value(q)? {
                return Err("command tapes differ".into());
            }
            println!("All actor command tapes match exactly");
        }
        ["convert", source, dest] => {
            let v: Value = read(Path::new(source))?;
            write(Path::new(dest), &v)?;
        }
        _ => return Err(USAGE.into()),
    }
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            std::process::ExitCode::FAILURE
        }
    }
}
