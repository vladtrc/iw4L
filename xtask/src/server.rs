use std::path::{Path, PathBuf};

use crate::dotenv::Env;
use crate::shell::{Res, Ssh};

pub fn root_dir(env: &Env) -> Res<PathBuf> {
    Ok(PathBuf::from(env.require("IW4L_GAMES")?))
}

pub struct Descriptor {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub community: community::Community,
}

impl Descriptor {
    pub fn read(path: &Path) -> Res<Self> {
        let bytes =
            std::fs::read(path).map_err(|error| format!("reading {}: {error}", path.display()))?;
        let community = community::Community::parse(&bytes)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        let descriptor = Self {
            path: path.to_path_buf(),
            bytes,
            community,
        };
        let port = descriptor.port()?;
        if !descriptor
            .community
            .updates
            .url
            .contains(&format!(":{port}/"))
        {
            return Err(format!(
                "{}: updates.url is not served on the master port {port}",
                path.display()
            ));
        }
        Ok(descriptor)
    }

    pub fn all_in(dir: &Path) -> Res<Vec<Self>> {
        let mut paths = std::fs::read_dir(dir)
            .map_err(|error| format!("reading {}: {error}", dir.display()))?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("reading {}: {error}", dir.display()))?;
        paths.retain(|path| path.extension().is_some_and(|ext| ext == "iw4l-server"));
        paths.sort();
        if paths.is_empty() {
            return Err(format!("no .iw4l-server files in {}", dir.display()));
        }
        paths.iter().map(|path| Self::read(path)).collect()
    }

    pub fn select(all: Vec<Self>, names: &[String]) -> Res<Vec<Self>> {
        if let Some(missing) = names
            .iter()
            .find(|name| !all.iter().any(|d| d.name() == name.as_str()))
        {
            let known = all.iter().map(Self::name).collect::<Vec<_>>().join(", ");
            return Err(format!("no server {missing:?}; have {known}"));
        }
        Ok(all
            .into_iter()
            .filter(|d| names.is_empty() || names.iter().any(|name| name == d.name()))
            .collect())
    }

    pub fn name(&self) -> &str {
        self.path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("")
    }

    pub fn file_name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("")
    }

    pub fn host(&self) -> &str {
        let address = &self.community.master.address;
        address.rsplit_once(':').map_or(address, |(host, _)| host)
    }

    pub fn port(&self) -> Res<u16> {
        self.community
            .master
            .address
            .rsplit_once(':')
            .and_then(|(_, port)| port.parse().ok())
            .ok_or_else(|| format!("{}: master.address has no port", self.path.display()))
    }

    pub fn server_name(&self) -> &str {
        &self.community.master.server_name
    }

    pub fn ca_pem(&self) -> &str {
        &self.community.updates.ca_pem
    }

    pub fn ssh(&self) -> Res<Ssh> {
        Ssh::new(&format!("root@{}", self.host()))
    }

    pub fn summary(&self) -> String {
        format!(
            "{} address={} server_name={} ca_sha256={}",
            self.name(),
            self.community.master.address,
            self.server_name(),
            &crate::release::sha256_hex(self.ca_pem().as_bytes())[..16],
        )
    }
}

pub struct Remote {
    pub unit: String,
    pub port: u16,
    pub bin: String,
    pub lib: String,
    pub etc: String,
    pub updates: String,
}

impl Remote {
    pub fn ca(&self) -> String {
        format!("{}/iw4l-ca.pem", self.etc)
    }

    pub fn for_name(name: &str, port: u16) -> Self {
        let lib = format!("/usr/local/lib/iw4l-{name}");
        Self {
            unit: format!("iw4l-master-{name}.service"),
            port,
            bin: format!("{lib}/iw4l-master"),
            updates: format!("{lib}/updates"),
            etc: format!("/etc/iw4l-{name}"),
            lib,
        }
    }

    pub fn find(ssh: &Ssh, port: u16) -> Res<Self> {
        let found = ssh.capture(&format!(
            "for f in /etc/systemd/system/*.service; do
              [ -f \"$f\" ] || continue
              line=$(grep -m1 '^ExecStart=.* serve --bind [^ ]*:{port} ' \"$f\" || true)
              [ -z \"$line\" ] || printf '%s\\t%s\\n' \"$(basename \"$f\")\" \"${{line#ExecStart=}}\"
            done"
        ))?;
        let mut units = found.lines().filter_map(|line| line.split_once('\t'));
        let (unit, exec) = units.next().ok_or_else(|| {
            format!(
                "no iw4l-master unit serves port {port} on {}; run cargo xtask master install",
                ssh.host()
            )
        })?;
        if units.next().is_some() {
            return Err(format!("several units serve port {port} on {}", ssh.host()));
        }
        let bin = exec
            .split_whitespace()
            .next()
            .ok_or_else(|| format!("{unit}: empty ExecStart"))?
            .to_string();
        let flag = |name: &str| {
            exec.split_whitespace()
                .skip_while(|word| *word != name)
                .nth(1)
                .map(str::to_string)
                .ok_or_else(|| format!("{unit}: ExecStart has no {name}"))
        };
        let parent = |path: &str| path.rsplit_once('/').map_or("", |(dir, _)| dir).to_string();
        Ok(Self {
            unit: unit.to_string(),
            port,
            lib: parent(&bin),
            etc: parent(&flag("--cert")?),
            updates: flag("--updates")?,
            bin,
        })
    }

    pub fn probe_command(&self, server_name: &str) -> String {
        format!(
            "'{bin}' status --connect 127.0.0.1:{port} --server-name '{server_name}' --ca-cert '{ca}'",
            bin = self.bin,
            port = self.port,
            ca = self.ca(),
        )
    }
}
