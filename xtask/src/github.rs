use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::release::{file_sha256, release_dir};
use crate::shell::{Res, capture, require_tools, run};

const ASSET: &str = "iw4l-windows.zip";

pub fn run_cli(root: &Path, args: &[String]) -> Res<()> {
    const USAGE: &str = "usage: cargo xtask github-release <tag> --notes FILE";
    let (tag, notes) = match args {
        [tag, flag, notes] if flag == "--notes" => (tag.as_str(), PathBuf::from(notes)),
        _ => return Err(USAGE.to_string()),
    };
    require_tools(&["gh", "git"])?;
    if !notes.is_file() {
        return Err(format!("missing notes {}", notes.display()));
    }
    let dir = release_dir(root)?;
    let deployment = dir.join("deployment.json");
    let deployment: Value = serde_json::from_str(
        &std::fs::read_to_string(&deployment)
            .map_err(|error| format!("reading {}: {error}", deployment.display()))?,
    )
    .map_err(|error| error.to_string())?;
    if deployment["dirty"] != false {
        return Err(format!("{} was built from a dirty tree", dir.display()));
    }
    let rev = deployment["git"]
        .as_str()
        .ok_or("deployment.json has no git")?;
    let git = |args: &[&str]| capture(Command::new("git").current_dir(root).args(args));
    let commit = git(&["rev-parse", &format!("{rev}^{{commit}}")])?
        .trim()
        .to_string();
    let remote = git(&["ls-remote", "origin", "refs/heads/main"])?;
    if remote.split_whitespace().next() != Some(commit.as_str()) {
        return Err(format!(
            "origin/main is not {commit}; push the release commit first"
        ));
    }

    let asset = dir.join(ASSET);
    let want = file_sha256(&asset)?;
    let view = |fields: &str| -> Res<Value> {
        let text = capture(Command::new("gh").args(["release", "view", tag, "--json", fields]))?;
        serde_json::from_str(&text).map_err(|error| error.to_string())
    };
    match view("isDraft") {
        Ok(existing) if existing["isDraft"] == false => {
            return Err(format!("{tag} is already public"));
        }
        Ok(_) => run(Command::new("gh")
            .args(["release", "upload", tag, "--clobber"])
            .arg(&asset))?,
        Err(_) => run(Command::new("gh")
            .args(["release", "create", tag, "--draft", "--prerelease"])
            .args(["--target", &commit, "--title", tag, "--notes-file"])
            .arg(&notes)
            .arg(&asset))?,
    }

    let assets = view("assets")?;
    let digest = assets["assets"]
        .as_array()
        .and_then(|assets| assets.iter().find(|a| a["name"] == ASSET))
        .and_then(|a| a["digest"].as_str())
        .ok_or("uploaded asset has no digest")?;
    if digest != format!("sha256:{want}") {
        return Err(format!("{ASSET} digest {digest} != local sha256:{want}"));
    }
    run(Command::new("gh").args(["release", "edit", tag, "--draft=false", "--prerelease"]))?;

    let tagged = git(&[
        "ls-remote",
        "origin",
        &format!("refs/tags/{tag}^{{}}"),
        &format!("refs/tags/{tag}"),
    ])?;
    if !tagged.lines().any(|line| line.starts_with(&commit)) {
        return Err(format!("tag {tag} does not point at {commit}"));
    }
    println!("github-release: {tag} commit={commit} asset sha256={want}");
    Ok(())
}
