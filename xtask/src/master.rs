use std::path::{Path, PathBuf};
use std::process::Command;

use crate::certs::{Ca, San};
use crate::dotenv::Env;
use crate::release::{build_master, file_sha256};
use crate::server::{Descriptor, Remote, root_dir};
use crate::shell::{Res, Ssh, Step, capture, require_tools};
use crate::windows;

const DEFAULT_SINCE: &str = "2h";

const JOURNALD_CONF: &str = "[Journal]\nSystemMaxUse=300M\nMaxRetentionSec=14day\n";

struct Install {
    ssh: Ssh,
    name: String,
    port: u16,
    ca: Ca,
}

fn parse_install(args: &[String]) -> Res<Install> {
    const USAGE: &str =
        "usage: cargo xtask master install user@host --name NAME --port PORT [--ca DIR]";
    let (mut target, mut name, mut port, mut ca_dir) = (None, None, None, None);
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let mut value = || rest.next().cloned().ok_or(USAGE);
        match arg.as_str() {
            "--name" => name = Some(value()?),
            "--port" => port = Some(value()?.parse().map_err(|_| USAGE)?),
            "--ca" => ca_dir = Some(PathBuf::from(value()?)),
            flag if flag.starts_with('-') => return Err(format!("unknown option {flag}")),
            value if target.is_none() => target = Some(value.to_string()),
            extra => return Err(format!("unexpected argument {extra}")),
        }
    }
    let name: String = name.ok_or(USAGE)?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        return Err(format!(
            "--name must be letters, digits and '-' (got {name:?})"
        ));
    }
    let ca_dir = match ca_dir {
        Some(dir) => dir,
        None => {
            let home = std::env::var("HOME").map_err(|_| "HOME is unset; pass --ca DIR")?;
            PathBuf::from(home).join(".iw4l").join(&name)
        }
    };
    Ok(Install {
        ssh: Ssh::new(&target.ok_or(USAGE)?)?,
        port: port.ok_or(USAGE)?,
        ca: Ca::new(ca_dir),
        name,
    })
}

struct Installed {
    server: Descriptor,
    ssh: Ssh,
    remote: Remote,
    since: String,
}

fn parse_installed(env: &Env, verb: &str, args: &[String]) -> Res<Installed> {
    let usage = format!("usage: cargo xtask master {verb} NAME [--since 2h]");
    let (name, since) = match args {
        [name] => (name, DEFAULT_SINCE.to_string()),
        [name, flag, since] if flag == "--since" => (name, since.clone()),
        _ => return Err(usage),
    };
    check_since(&since)?;
    let server = Descriptor::select(
        Descriptor::all_in(&root_dir(env)?)?,
        std::slice::from_ref(name),
    )?
    .pop()
    .ok_or(usage)?;
    let ssh = server.ssh()?;
    let remote = Remote::find(&ssh, server.port()?)?;
    Ok(Installed {
        server,
        ssh,
        remote,
        since,
    })
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
fn unit_text(root: &Path, name: &str, remote: &Remote, owner: (&str, &str)) -> Res<String> {
    let (user, group) = owner;
    capture(
        Command::new("cargo")
            .current_dir(root)
            .args(["run", "--quiet", "-p", "iw4l-master", "--", "print-unit"])
            .args(["--name", name])
            .args(["--port", &remote.port.to_string()])
            .args(["--exec", &remote.bin])
            .args(["--cert", &format!("{}/server-cert.pem", remote.etc)])
            .args(["--key", &format!("{}/server-key.pem", remote.etc)])
            .args(["--updates", &remote.updates])
            .args(["--user", user])
            .args(["--group", group]),
    )
}

fn upload_binary(root: &Path, env: &Env, ssh: &Ssh, remote: &Remote) -> Res<bool> {
    let profile = windows::profile(env)?;
    let bin = build_master(root, &profile)?;
    let installed = ssh
        .capture(&format!(
            "if [ -f '{0}' ]; then sha256sum '{0}' | cut -d' ' -f1; fi",
            remote.bin
        ))?
        .trim()
        .to_string();
    if installed == file_sha256(&bin)? {
        println!("master.upload: unchanged sha256={installed}");
        return Ok(false);
    }
    let step = Step::start("master.upload", ssh.target());
    ssh.run(&format!("install -d -m 0755 '{}'", remote.lib))?;
    ssh.rsync(&["--chmod=F755"], &bin, &remote.bin)?;
    step.done("");
    Ok(true)
}

fn upload_certs(ca: &Ca, ssh: &Ssh, remote: &Remote) -> Res<()> {
    let etc = &remote.etc;
    ssh.run(&format!("install -d -m 0755 '{etc}'"))?;
    ssh.rsync(&["--chmod=F644"], &ca.ca_cert(), &remote.ca())?;
    ssh.rsync(
        &["--chmod=F644"],
        &ca.server_cert(),
        &format!("{etc}/server-cert.pem"),
    )?;
    ssh.rsync(
        &["--chmod=F640"],
        &ca.server_key(),
        &format!("{etc}/server-key.pem"),
    )?;
    // Readable by the service account and by nobody else on the box.
    ssh.run(&format!(
        "chown root:iw4l '{etc}/server-key.pem' && chmod 0640 '{etc}/server-key.pem'"
    ))
}

pub fn install(root: &Path, env: &Env, args: &[String]) -> Res<()> {
    let Install {
        ssh,
        name,
        port,
        ca,
    } = parse_install(args)?;
    let remote = Remote::for_name(&name, port);
    let label = format!("iw4l-{name}");
    let descriptor = root_dir(env)?.join(format!("{name}.iw4l-server"));
    require_tools(&["cargo", "rsync", "ssh"])?;
    if let Ok(existing) = Remote::find(&ssh, port)
        && existing.unit != remote.unit
    {
        return Err(format!("{} already serves port {port}", existing.unit));
    }
    ca.ensure(&San {
        label: label.clone(),
        host: ssh.host().to_owned(),
    })?;

    let step = Step::start(
        "master.install",
        &format!("host={} name={name} port={port}", ssh.target()),
    );
    ssh.run(
        "set -eu
        getent group iw4l >/dev/null || groupadd --system iw4l
        id iw4l >/dev/null 2>&1 || useradd --system --gid iw4l --home-dir /nonexistent --shell /usr/sbin/nologin iw4l",
    )?;
    upload_binary(root, env, &ssh, &remote)?;
    upload_certs(&ca, &ssh, &remote)?;

    let unit = unit_text(root, &name, &remote, ("iw4l", "iw4l"))?;
    ssh.feed(
        &format!("cat >'/etc/systemd/system/{}'", remote.unit),
        &unit,
    )?;
    ssh.run("install -d -m 0755 /etc/systemd/journald.conf.d")?;
    ssh.feed("cat >/etc/systemd/journald.conf.d/iw4l.conf", JOURNALD_CONF)?;
    ssh.run(&format!(
        "set -eu
        if command -v ufw >/dev/null && ufw status | grep -q '^Status: active'; then
          ufw allow {port}/udp
          ufw allow {port}/tcp
        fi
        systemctl daemon-reload
        systemctl restart systemd-journald
        systemctl enable '{unit}'
        systemctl restart '{unit}'",
        unit = remote.unit,
    ))?;
    step.done("");
    report(&ssh, &remote, &label)?;
    hand_out(&ssh, &name, port, &label, &ca, &descriptor)
}

pub fn update(root: &Path, env: &Env, args: &[String]) -> Res<()> {
    let Installed {
        server,
        ssh,
        remote,
        ..
    } = parse_installed(env, "update", args)?;
    require_tools(&["cargo", "rsync", "ssh"])?;
    if upload_binary(root, env, &ssh, &remote)? {
        ssh.run(&format!("systemctl restart '{}'", remote.unit))?;
    }
    report(&ssh, &remote, server.server_name())
}

pub fn status(env: &Env, args: &[String]) -> Res<()> {
    let Installed {
        server,
        ssh,
        remote,
        ..
    } = parse_installed(env, "status", args)?;
    report(&ssh, &remote, server.server_name())
}

pub fn logs(env: &Env, args: &[String]) -> Res<()> {
    let Installed {
        ssh, remote, since, ..
    } = parse_installed(env, "logs", args)?;
    journal(&ssh, &remote.unit, &since)
}

pub fn uninstall(env: &Env, args: &[String]) -> Res<()> {
    let Installed { ssh, remote, .. } = parse_installed(env, "uninstall", args)?;
    ssh.run(&format!(
        "set -eu
        systemctl disable --now '{unit}' 2>/dev/null || true
        rm -f '/etc/systemd/system/{unit}'
        systemctl daemon-reload
        rm -f '{bin}'",
        unit = remote.unit,
        bin = remote.bin,
    ))?;
    println!(
        "master: {} removed from {}. Certificates under {} and the local CA were kept.",
        remote.unit,
        ssh.target(),
        remote.etc,
    );
    Ok(())
}

pub fn journal(ssh: &Ssh, unit: &str, since: &str) -> Res<()> {
    ssh.run(&format!(
        "journalctl -u '{unit}' --since '-{since}' --no-pager"
    ))
}

fn report(ssh: &Ssh, remote: &Remote, server_name: &str) -> Res<()> {
    ssh.run(&format!(
        "systemctl --no-pager --full status '{}' || true",
        remote.unit
    ))?;
    ssh.run(&remote.probe_command(server_name))
}

fn hand_out(ssh: &Ssh, name: &str, port: u16, label: &str, ca: &Ca, path: &Path) -> Res<()> {
    if path.exists() {
        println!("master: kept existing {}", path.display());
        return Ok(());
    }
    let host = ssh.host();
    let descriptor = community::Community {
        schema: 1,
        name: format!("IW4L {name}"),
        master: community::Master {
            address: format!("{host}:{port}"),
            server_name: label.into(),
        },
        updates: community::Updates {
            url: format!("https://{host}:{port}/updates/manifest.toml"),
            ca_pem: std::fs::read_to_string(ca.ca_cert()).map_err(|e| e.to_string())?,
        },
    };
    std::fs::write(
        path,
        toml::to_string_pretty(&descriptor).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("writing {}: {e}", path.display()))?;
    println!(
        "master: wrote {}; `make deploy {name}` publishes to it.",
        path.display()
    );
    Ok(())
}

pub fn run_cli(root: &Path, env: &Env, args: &[String]) -> Res<()> {
    let (verb, rest) = args
        .split_first()
        .ok_or("usage: cargo xtask master <install user@host|update|status|logs|uninstall NAME>")?;
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
