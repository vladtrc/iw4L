//! `cargo xtask master …` — a relay of your own on a VPS you rent, installed
//! over ssh from the machine holding the clone. `docs/MASTER.md` is the prose.
//!
//! Nothing here is our release pipeline: no Caddy, no update URL, no channels
//! to publish between. One binary, one certificate, one unit — and the CA
//! private key never leaves this machine.

use std::path::{Path, PathBuf};
use std::process::Command;

use master_protocol::Channel;

use crate::certs::{Ca, San};
use crate::dotenv::Env;
use crate::release::build_master;
use crate::shell::{Res, Ssh, Step, capture, require_tools};
use crate::windows;

/// Where the binary and its certificates land on the VPS. `/usr/local/lib`
/// rather than `/usr/local/bin`: nobody runs this by hand, systemd does.
const REMOTE_LIB: &str = "/usr/local/lib/iw4l";
const REMOTE_ETC: &str = "/etc/iw4l";

const DEFAULT_SINCE: &str = "2h";

struct Args {
    ssh: Ssh,
    channel: Channel,
    ca: Ca,
    since: String,
}

fn parse(env: &Env, args: &[String]) -> Res<Args> {
    let mut target = None;
    let mut channel = Channel::Prod;
    let mut since = DEFAULT_SINCE.to_string();
    let mut ca_dir = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--channel" => {
                channel = rest
                    .next()
                    .ok_or("--channel needs prod or dev")?
                    .parse()
                    .map_err(|_| "--channel needs prod or dev".to_string())?;
            }
            "--since" => {
                since = rest.next().ok_or("--since needs a journal window")?.clone();
            }
            "--ca" => {
                ca_dir = Some(PathBuf::from(rest.next().ok_or("--ca needs a directory")?));
            }
            flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            value if target.is_none() => target = Some(value.to_string()),
            extra => return Err(format!("unexpected argument {extra}")),
        }
    }
    check_since(&since)?;
    let target = target.ok_or("usage: cargo xtask master <verb> user@host")?;
    let ca_dir = match ca_dir {
        Some(dir) => dir,
        None => default_ca_dir(env)?,
    };
    Ok(Args {
        ssh: Ssh::new(&target)?,
        channel,
        ca: Ca::new(ca_dir),
        since,
    })
}

/// `~/.iw4l/ca` unless told otherwise. Deliberately outside the clone: the
/// private key of everybody's trust anchor is not a repository file.
fn default_ca_dir(env: &Env) -> Res<PathBuf> {
    if let Some(dir) = env.get("IW4L_MASTER_CA_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var("HOME").map_err(|_| "HOME is unset; pass --ca DIR")?;
    Ok(PathBuf::from(home).join(".iw4l/ca"))
}

pub fn check_since(since: &str) -> Res<()> {
    let digits = since.trim_end_matches(|c: char| c.is_ascii_alphabetic());
    let unit = &since[digits.len()..];
    let ok = !digits.is_empty()
        && digits.bytes().all(|b| b.is_ascii_digit())
        && matches!(
            unit,
            "min" | "mins" | "h" | "hour" | "hours" | "day" | "days" | "week" | "weeks"
        );
    if ok {
        Ok(())
    } else {
        Err(format!(
            "--since must look like 30min, 2h, 1day or 1week (got {since:?})"
        ))
    }
}

/// The unit text, printed by the binary that parses `serve`. Asking the binary
/// is the whole point: an `ExecStart` written here could drift from the flags
/// `serve` actually accepts, and this one cannot.
pub fn unit_text(
    root: &Path,
    channel: Channel,
    exec: &str,
    cert: &str,
    key: &str,
    user: &str,
    group: &str,
) -> Res<String> {
    capture(
        Command::new("cargo")
            .current_dir(root)
            .args(["run", "--quiet", "-p", "iw4l-master", "--", "print-unit"])
            .args(["--channel", channel.as_str()])
            .args(["--exec", exec])
            .args(["--cert", cert])
            .args(["--key", key])
            .args(["--user", user])
            .args(["--group", group]),
    )
}

fn remote_bin() -> String {
    format!("{REMOTE_LIB}/iw4l-master")
}

fn remote_ca() -> String {
    format!("{REMOTE_ETC}/iw4l-ca.pem")
}

/// Build the static binary and put it on the VPS. `crt-static` is why the
/// glibc version over there does not have to match this machine's.
fn upload_binary(root: &Path, env: &Env, ssh: &Ssh) -> Res<()> {
    let profile = windows::profile(env)?;
    let bin = build_master(root, &profile)?;
    let step = Step::start("master.upload", ssh.target());
    ssh.run(&format!("install -d -m 0755 '{REMOTE_LIB}'"))?;
    ssh.rsync(&["--chmod=F755"], &bin, &remote_bin())?;
    step.done("");
    Ok(())
}

fn upload_certs(ca: &Ca, ssh: &Ssh) -> Res<()> {
    ssh.run(&format!("install -d -m 0755 '{REMOTE_ETC}'"))?;
    ssh.rsync(&["--chmod=F644"], &ca.ca_cert(), &remote_ca())?;
    ssh.rsync(
        &["--chmod=F644"],
        &ca.server_cert(),
        &format!("{REMOTE_ETC}/server-cert.pem"),
    )?;
    ssh.rsync(
        &["--chmod=F640"],
        &ca.server_key(),
        &format!("{REMOTE_ETC}/server-key.pem"),
    )?;
    // Readable by the service account and by nobody else on the box.
    ssh.run(&format!(
        "chown root:iw4l '{REMOTE_ETC}/server-key.pem' && chmod 0640 '{REMOTE_ETC}/server-key.pem'"
    ))
}

pub fn install(root: &Path, env: &Env, args: &[String]) -> Res<()> {
    let Args {
        ssh, channel, ca, ..
    } = parse(env, args)?;
    require_tools(&["cargo", "rsync", "ssh"])?;
    // No host in the SAN: the client checks the fixed label, so this one
    // certificate follows you to a new IP or a new VPS.
    ca.ensure(&San::Labels)?;

    let step = Step::start(
        "master.install",
        &format!("host={} channel={channel}", ssh.target()),
    );
    ssh.run(
        "set -eu
        getent group iw4l >/dev/null || groupadd --system iw4l
        id iw4l >/dev/null 2>&1 || useradd --system --gid iw4l --home-dir /nonexistent --shell /usr/sbin/nologin iw4l",
    )?;
    upload_binary(root, env, &ssh)?;
    upload_certs(&ca, &ssh)?;

    let unit = unit_text(
        root,
        channel,
        &remote_bin(),
        &format!("{REMOTE_ETC}/server-cert.pem"),
        &format!("{REMOTE_ETC}/server-key.pem"),
        "iw4l",
        "iw4l",
    )?;
    ssh.feed(
        &format!("cat >'/etc/systemd/system/{}'", channel.unit()),
        &unit,
    )?;
    ssh.run(&format!(
        "set -eu
        if command -v ufw >/dev/null && ufw status | grep -q '^Status: active'; then
          ufw allow {port}/udp
        fi
        systemctl daemon-reload
        systemctl enable --now '{unit}'",
        port = channel.port(),
        unit = channel.unit(),
    ))?;
    step.done("");
    report(&ssh, channel)?;
    hand_out(&ssh, channel, &ca);
    Ok(())
}

/// Rebuild, upload, restart. The certificates and the unit are left alone.
pub fn update(root: &Path, env: &Env, args: &[String]) -> Res<()> {
    let Args { ssh, channel, .. } = parse(env, args)?;
    require_tools(&["cargo", "rsync", "ssh"])?;
    upload_binary(root, env, &ssh)?;
    ssh.run(&format!("systemctl restart '{}'", channel.unit()))?;
    report(&ssh, channel)
}

pub fn status(env: &Env, args: &[String]) -> Res<()> {
    let Args { ssh, channel, .. } = parse(env, args)?;
    report(&ssh, channel)
}

pub fn logs(env: &Env, args: &[String]) -> Res<()> {
    let Args {
        ssh,
        channel,
        since,
        ..
    } = parse(env, args)?;
    journal(&ssh, channel, &since)
}

/// Stop and forget the service: its unit and its binary go. The certificates
/// stay on both ends — a reinstall has to keep the trust anchor every player
/// already pinned.
pub fn uninstall(env: &Env, args: &[String]) -> Res<()> {
    let Args { ssh, channel, .. } = parse(env, args)?;
    ssh.run(&format!(
        "set -eu
        systemctl disable --now '{unit}' 2>/dev/null || true
        rm -f '/etc/systemd/system/{unit}'
        systemctl daemon-reload
        rm -f '{bin}'
        rmdir '{REMOTE_LIB}' 2>/dev/null || true",
        unit = channel.unit(),
        bin = remote_bin(),
    ))?;
    println!(
        "master: {} removed from {}. Certificates under {REMOTE_ETC} and the local CA were kept.",
        channel.unit(),
        ssh.target()
    );
    Ok(())
}

pub fn journal(ssh: &Ssh, channel: Channel, since: &str) -> Res<()> {
    ssh.run(&format!(
        "journalctl -u '{unit}' --since '-{since}' --no-pager",
        unit = channel.unit(),
    ))
}

fn report(ssh: &Ssh, channel: Channel) -> Res<()> {
    ssh.run(&format!(
        "systemctl --no-pager --full status '{unit}' || true",
        unit = channel.unit()
    ))?;
    ssh.run(&format!(
        "'{bin}' status --connect 127.0.0.1:{port} --server-name '{name}' --ca-cert '{ca}'",
        bin = remote_bin(),
        port = channel.port(),
        name = channel.server_name(),
        ca = remote_ca(),
    ))
}

/// The three lines a player needs, plus the file they have to be given by
/// hand. The label is the same for everyone, so `ca.pem` is the whole of the
/// trust: hand it over a channel the players already trust.
fn hand_out(ssh: &Ssh, channel: Channel, ca: &Ca) {
    println!();
    println!(
        "Give this .env block and {} to your players:",
        ca.ca_cert().display()
    );
    println!();
    println!("  IW4L_MASTER_ADDR={}:{}", ssh.host(), channel.port());
    println!("  IW4L_MASTER_SERVER_NAME={}", channel.server_name());
    println!("  IW4L_MASTER_CA_CERT=/path/to/iw4l-ca.pem");
    println!();
    println!("  host a match:  IW4L_MASTER_HOST_NAME='name' make map mp_boneyard");
    println!("  join:          make menu");
}

pub fn run_cli(root: &Path, env: &Env, args: &[String]) -> Res<()> {
    let (verb, rest) = args
        .split_first()
        .ok_or("usage: cargo xtask master <install|update|status|logs|uninstall> user@host")?;
    match verb.as_str() {
        "install" => install(root, env, rest),
        "update" => update(root, env, rest),
        "status" => status(env, rest),
        "logs" => logs(env, rest),
        "uninstall" => uninstall(env, rest),
        other => Err(format!(
            "unknown master verb {other}; expected install|update|status|logs|uninstall"
        )),
    }
}
