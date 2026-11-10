use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};

use master_protocol::{
    ContentFamilies, ControlFrame, ControlHello, ControlRequest, EndpointRole, RequestBody,
    ResponseBody,
};
use serde_json::{Value, json};

type Result<T> = std::result::Result<T, String>;

pub(super) struct Master {
    process: Child,
    pub descriptor: PathBuf,
    runtime: tokio::runtime::Runtime,
    address: SocketAddr,
    ca: String,
    hosts: Vec<Control>,
}

impl Master {
    pub fn start(binary: &Path, directory: &Path) -> Result<Self> {
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        fs::write(directory.join("server.ext"), "subjectAltName=DNS:localhost\nbasicConstraints=CA:FALSE\nkeyUsage=digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n").map_err(|e| e.to_string())?;
        let commands: &[&[&str]] = &[
            &[
                "req",
                "-x509",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-days",
                "1",
                "-subj",
                "/CN=Approved fixture CA",
                "-keyout",
                "ca.key",
                "-out",
                "ca.pem",
            ],
            &[
                "req",
                "-newkey",
                "rsa:2048",
                "-nodes",
                "-subj",
                "/CN=localhost",
                "-keyout",
                "server.key",
                "-out",
                "server.csr",
            ],
            &[
                "x509",
                "-req",
                "-in",
                "server.csr",
                "-CA",
                "ca.pem",
                "-CAkey",
                "ca.key",
                "-CAcreateserial",
                "-days",
                "1",
                "-extfile",
                "server.ext",
                "-out",
                "server.pem",
            ],
        ];
        let mut certificate_log = Vec::new();
        for args in commands {
            let output = Command::new("openssl")
                .args(*args)
                .current_dir(directory)
                .output()
                .map_err(|e| e.to_string())?;
            certificate_log.extend_from_slice(&output.stderr);
            fs::write(directory.join("certificate.log"), &certificate_log)
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err("cannot issue local master certificate".into());
            }
        }
        let socket = std::net::UdpSocket::bind("127.0.0.1:0").map_err(|e| e.to_string())?;
        let address = socket.local_addr().map_err(|e| e.to_string())?;
        drop(socket);
        fs::create_dir(directory.join("updates")).map_err(|e| e.to_string())?;
        let process = Command::new(binary)
            .arg("serve")
            .arg("--bind")
            .arg(address.to_string())
            .arg("--cert")
            .arg(directory.join("server.pem"))
            .arg("--key")
            .arg(directory.join("server.key"))
            .arg("--updates")
            .arg(directory.join("updates"))
            .stdout(fs::File::create(directory.join("stdout.log")).map_err(|e| e.to_string())?)
            .stderr(fs::File::create(directory.join("stderr.log")).map_err(|e| e.to_string())?)
            .stdin(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        let ca = fs::read_to_string(directory.join("ca.pem")).map_err(|e| e.to_string())?;
        let descriptor = directory.join("local.iw4l-server");
        let community = community::Community {
            schema: 1,
            name: "Approved local master".into(),
            master: community::Master {
                address: address.to_string(),
                server_name: "localhost".into(),
            },
            updates: community::Updates {
                url: format!("https://localhost:{}/updates/manifest.toml", address.port()),
                ca_pem: ca.clone(),
            },
        };
        fs::write(
            &descriptor,
            toml::to_string(&community).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .map_err(|e| e.to_string())?;
        let mut master = Self {
            process,
            descriptor,
            runtime,
            address,
            ca,
            hosts: Vec::new(),
        };
        let start = Instant::now();
        loop {
            if let Some(status) = master.process.try_wait().map_err(|e| e.to_string())? {
                return Err(format!("local master exited: {status}"));
            }
            if fs::read_to_string(directory.join("stderr.log"))
                .is_ok_and(|s| s.contains("listening on"))
            {
                break;
            }
            if start.elapsed() > Duration::from_secs(20) {
                return Err("local master did not start".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(master)
    }

    pub fn large_rooms(&mut self) -> Result<Value> {
        let (hosts, evidence) = self.runtime.block_on(async {
            let families = ContentFamilies::from_families(std::iter::once("iw4".to_owned())
                .chain((0..ContentFamilies::MAX_FAMILIES - 1).map(|i| format!("f{i:063}")))).map_err(|e| e.to_string())?;
            let mut hosts = Vec::new();
            let mut ids = Vec::new();
            for index in 0..2 {
                let mut host = Control::connect(self.address, &self.ca, EndpointRole::Host).await?;
                let response = host.rpc(RequestBody::CreateRoom {
                    name: format!("boundary-{index}-{}", "n".repeat(master_protocol::MAX_ADVERT_NAME_BYTES - 11)),
                    map: format!("iw4:{}", "m".repeat(master_protocol::MAX_MAP_BYTES - 4)),
                    mode: "d".repeat(master_protocol::MAX_MODE_BYTES), max_players: master_protocol::MAX_SESSION_MEMBERS,
                    requires: families.clone(), available: families.clone(), password: String::new(),
                }).await?;
                let ResponseBody::RoomCreated { view, .. } = response else { return Err(format!("large room rejected: {response:?}")); };
                ids.push(view.room_id);
                hosts.push(host);
            }
            let mut reader = Control::connect(self.address, &self.ca, EndpointRole::Cli).await?;
            let mut after = None;
            let mut pages = Vec::new();
            let mut listed = Vec::new();
            loop {
                let response = reader.rpc(RequestBody::ListRooms { after }).await?;
                let frame = ControlFrame::Response(master_protocol::ControlResponse { request_id: reader.serial, body: response.clone() });
                let encoded = master_protocol::encode_stream_frame(&frame).map_err(|e| e.to_string())?;
                let ResponseBody::RoomList { adverts, next, .. } = response else { return Err(format!("list rejected: {response:?}")); };
                if adverts.len() != 1 || encoded.len() > master_protocol::MAX_CONTROL_BYTES + 4 { return Err("large room pagination lost its byte boundary".into()); }
                if adverts.iter().any(|a| a.requires != families || a.available != families) { return Err("large inventories changed in transit".into()); }
                listed.extend(adverts.iter().map(|a| a.id));
                pages.push(json!({ "bytes": encoded.len(), "adverts": adverts.len(), "next": next.map(|id| id.to_string()) }));
                if next.is_none() { break; }
                if after == next || pages.len() > 2 { return Err("pagination cursor did not terminate".into()); }
                after = next;
            }
            ids.sort(); listed.sort();
            if ids != listed || pages.len() != 2 { return Err("large rooms missing or duplicated".into()); }
            if !matches!(reader.rpc(RequestBody::Status).await?, ResponseBody::Status(_)) { return Err("reader disconnected after pagination".into()); }
            Ok((hosts, json!({ "ids": ids.iter().map(ToString::to_string).collect::<Vec<_>>(), "families": families.iter().collect::<Vec<_>>(), "pages": pages, "status_after_list": true })))
        })?;
        self.hosts = hosts;
        Ok(evidence)
    }

    pub fn close_large_rooms(&mut self) -> Result<()> {
        self.runtime.block_on(async {
            for host in &mut self.hosts {
                if !matches!(host.rpc(RequestBody::CloseRoom).await?, ResponseBody::Ack) {
                    return Err::<(), String>("cannot close fixture room".into());
                }
            }
            Ok(())
        })?;
        self.hosts.clear();
        Ok(())
    }
}

impl Drop for Master {
    fn drop(&mut self) {
        for host in &self.hosts {
            host.connection.close(0_u8.into(), b"fixture complete");
        }
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

struct Control {
    _endpoint: quinn::Endpoint,
    connection: quinn::Connection,
    send: quinn::SendStream,
    recv: quinn::RecvStream,
    serial: u64,
}

impl Control {
    async fn connect(address: SocketAddr, ca: &str, role: EndpointRole) -> Result<Self> {
        let mut crypto = rustls::ClientConfig::builder()
            .with_root_certificates(community::trust_roots(ca)?)
            .with_no_client_auth();
        crypto.alpn_protocols = vec![master_protocol::ALPN.to_vec()];
        let crypto =
            quinn::crypto::rustls::QuicClientConfig::try_from(crypto).map_err(|e| e.to_string())?;
        let mut endpoint = quinn::Endpoint::client(
            "127.0.0.1:0"
                .parse()
                .map_err(|e: std::net::AddrParseError| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(crypto)));
        let connection = tokio::time::timeout(
            Duration::from_secs(10),
            endpoint
                .connect(address, "localhost")
                .map_err(|e| e.to_string())?,
        )
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
        let (send, recv) = connection.open_bi().await.map_err(|e| e.to_string())?;
        let mut control = Self {
            _endpoint: endpoint,
            connection,
            send,
            recv,
            serial: 0,
        };
        control
            .write(&ControlFrame::Hello(ControlHello {
                protocol_version: master_protocol::PROTOCOL_VERSION,
                game_protocol: 0,
                role,
                build: "approved_tests".into(),
                player_name: String::new(),
            }))
            .await?;
        Ok(control)
    }

    async fn write(&mut self, frame: &ControlFrame) -> Result<()> {
        self.send
            .write_all(&master_protocol::encode_stream_frame(frame).map_err(|e| e.to_string())?)
            .await
            .map_err(|e| e.to_string())
    }

    async fn rpc(&mut self, body: RequestBody) -> Result<ResponseBody> {
        self.serial += 1;
        self.write(&ControlFrame::Request(ControlRequest {
            request_id: self.serial,
            body,
        }))
        .await?;
        tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                let mut header = [0; 4];
                self.recv
                    .read_exact(&mut header)
                    .await
                    .map_err(|e| e.to_string())?;
                let length =
                    master_protocol::stream_frame_len(header).map_err(|e| e.to_string())?;
                let mut bytes = vec![0; length];
                self.recv
                    .read_exact(&mut bytes)
                    .await
                    .map_err(|e| e.to_string())?;
                match master_protocol::decode_stream_payload(&bytes).map_err(|e| e.to_string())? {
                    ControlFrame::Response(response) if response.request_id == self.serial => {
                        return Ok(response.body);
                    }
                    ControlFrame::RoomView(_)
                    | ControlFrame::Closed { .. }
                    | ControlFrame::PeerEvent(_)
                    | ControlFrame::Relay(_) => {}
                    other => return Err(format!("unexpected master frame: {other:?}")),
                }
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
