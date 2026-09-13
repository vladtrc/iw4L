//! Our release host: users, directories, packages, systemd, Caddy, firewall
//! and the server certificate. Behind `make provision`.
//!
//! An ordinary `cargo xtask publish` never runs any of this — it rewrites
//! nothing under `/etc` — which is the reason the two are separate commands.

use std::path::Path;

use master_protocol::Channel;

use crate::certs::{Ca, San};
use crate::dotenv::Env;
use crate::master::unit_text;
use crate::shell::{Res, Ssh, Step, require_tools};

/// The HTTPS port Caddy serves `releases/` on. The updater's `IW4L_UPDATE_URL`
/// is written with it in `cargo xtask release`.
const RELEASE_HTTPS_PORT: u16 = 8443;

fn caddyfile(deploy_root: &str) -> String {
    format!(
        ":{RELEASE_HTTPS_PORT} {{\n    \
         tls /etc/iw4l/server-cert.pem /etc/iw4l/server-key.pem\n    \
         root * {deploy_root}/releases\n    \
         file_server\n}}\n"
    )
}

/// Two weeks of a bounded journal: enough to answer "what happened last
/// weekend", small enough that the smallest VPS disk never fills.
const JOURNALD_CONF: &str = "[Journal]\nSystemMaxUse=300M\nMaxRetentionSec=14day\n";

pub fn run_cli(root: &Path, env: &Env, _args: &[String]) -> Res<()> {
    require_tools(&["cargo", "openssl", "rsync", "ssh"])?;
    let ssh = Ssh::new(&env.require("IW4L_DEPLOY_HOST")?)?;
    let deploy_root = env.require("IW4L_DEPLOY_ROOT")?;
    crate::shell::check_deploy_root(&deploy_root)?;
    let ca = Ca::new(env.require("IW4L_RELEASE_KEY")?.into());

    // Caddy is verified by hostname, so this certificate — unlike a master of
    // your own — does need the host in its SAN.
    ca.ensure(&San::WithHost(ssh.host().to_string()))?;

    let step = Step::start(
        "provision",
        &format!("host={} root={deploy_root}", ssh.target()),
    );
    ssh.run(&format!(
        "set -eu
        if ! command -v caddy >/dev/null || ! command -v rsync >/dev/null; then
          apt-get update
          DEBIAN_FRONTEND=noninteractive apt-get install -y caddy rsync
        fi
        getent group iw4l-release >/dev/null || groupadd --system iw4l-release
        id iw4l >/dev/null 2>&1 || useradd --system --gid iw4l-release --home-dir '{deploy_root}' --shell /usr/sbin/nologin iw4l
        usermod -a -G iw4l-release caddy
        install -d -m 0755 \
          '{deploy_root}/bin' \
          '{deploy_root}/masters' \
          '{deploy_root}/releases/dev/manifests' \
          '{deploy_root}/releases/prod/manifests' \
          '{deploy_root}/staging/dev' \
          '{deploy_root}/staging/prod' \
          '{deploy_root}/locks' \
          /etc/iw4l /etc/systemd/system /etc/systemd/journald.conf.d"
    ))?;

    for channel in Channel::ALL {
        let unit = unit_text(
            root,
            channel,
            &format!("{deploy_root}/bin/iw4l-master-{channel}"),
            "/etc/iw4l/server-cert.pem",
            "/etc/iw4l/server-key.pem",
            "iw4l",
            "iw4l-release",
        )?;
        ssh.feed(
            &format!("cat >'/etc/systemd/system/{}'", channel.unit()),
            &unit,
        )?;
    }
    ssh.feed("cat >/etc/caddy/Caddyfile", &caddyfile(&deploy_root))?;
    ssh.feed("cat >/etc/systemd/journald.conf.d/iw4l.conf", JOURNALD_CONF)?;
    ssh.rsync(&["--chmod=F644"], &ca.ca_cert(), "/etc/iw4l/iw4l-ca.pem")?;
    ssh.rsync(
        &["--chmod=F644"],
        &ca.server_cert(),
        "/etc/iw4l/server-cert.pem",
    )?;
    ssh.rsync(
        &["--chmod=F640"],
        &ca.server_key(),
        "/etc/iw4l/server-key.pem",
    )?;

    let ufw_ports = Channel::ALL
        .iter()
        .map(|channel| format!("ufw allow {}/udp", channel.port()))
        .collect::<Vec<_>>()
        .join("\n          ");
    let units = Channel::ALL
        .iter()
        .map(|channel| format!("'{}'", channel.unit()))
        .collect::<Vec<_>>()
        .join(" ");
    ssh.run(&format!(
        "set -eu
        systemctl disable --now iw4l-dedicated@prod.service iw4l-dedicated@dev.service 2>/dev/null || true
        rm -f /etc/systemd/system/iw4l-dedicated@.service \
          '{deploy_root}/bin/iw4l-dedicated-prod' \
          '{deploy_root}/bin/iw4l-dedicated-dev' \
          '{deploy_root}/config/prod.env' \
          '{deploy_root}/config/dev.env'
        chown root:iw4l-release /etc/iw4l/server-key.pem
        chmod 640 /etc/iw4l/server-key.pem
        caddy validate --config /etc/caddy/Caddyfile
        systemctl daemon-reload
        systemctl restart systemd-journald caddy
        if command -v ufw >/dev/null && ufw status | grep -q '^Status: active'; then
          {ufw_ports}
          ufw allow {RELEASE_HTTPS_PORT}/tcp
        fi
        systemctl enable caddy >/dev/null
        systemctl enable {units} >/dev/null
        # Do not start a master here: its executable symlink appears on first publish."
    ))?;

    ssh.run("systemctl --no-pager --full status caddy")?;
    step.done("");
    println!(
        "provision: Caddy/journald/units updated; a master is started by `cargo xtask publish` when its binary changes"
    );
    Ok(())
}
