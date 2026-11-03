use super::*;
use frame::AppScreen;
use tokio::sync::watch;

#[derive(Resource)]
pub struct CommunityPlayerCounts {
    counts: Arc<Mutex<Vec<Option<u32>>>>,
    presence: watch::Sender<Presence>,
    cancel: CancellationToken,
}

impl CommunityPlayerCounts {
    pub fn snapshot(&self) -> Option<Vec<Option<u32>>> {
        self.counts.try_lock().ok().map(|counts| counts.clone())
    }
}

impl Drop for CommunityPlayerCounts {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Presence {
    menu: bool,
    online: bool,
}

impl Presence {
    fn wanted(self, selected: bool) -> bool {
        self.menu || (selected && self.online)
    }
}

pub(super) fn poll_communities(
    mut commands: Commands,
    counts: Option<Res<CommunityPlayerCounts>>,
    bridge: Option<Res<MasterBridge>>,
    screen: Option<Res<AppScreen>>,
) {
    let presence = Presence {
        menu: screen.is_none_or(|screen| *screen == AppScreen::MainMenu),
        online: bridge.as_ref().is_none_or(|bridge| {
            !matches!(
                bridge.state(),
                MasterBridgeState::Hosting { .. } | MasterBridgeState::Joined { .. }
            )
        }),
    };
    if let Some(counts) = counts {
        counts
            .presence
            .send_if_modified(|current| std::mem::replace(current, presence) != presence);
        return;
    }
    let (presence, watcher) = watch::channel(presence);
    let counts = CommunityPlayerCounts {
        counts: Arc::new(Mutex::new(vec![None; community::communities().len()])),
        presence,
        cancel: CancellationToken::new(),
    };
    let communities: Vec<_> = community::communities()
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let community = std::fs::read(&entry.path)
                .ok()
                .and_then(|bytes| community::Community::parse(&bytes).ok())?;
            let target = MasterTarget {
                address: community.master.address,
                server_name: community.master.server_name,
                ca_pem: community.updates.ca_pem,
            };
            let selected = Some(entry.path.as_path()) == community::selected_path();
            Some((index, target, selected))
        })
        .collect();
    if !communities.is_empty() {
        let state = Arc::clone(&counts.counts);
        let cancel = counts.cancel.clone();
        std::thread::Builder::new()
            .name("iw4l-population".into())
            .spawn(move || {
                let Ok(runtime) = blocking_runtime() else {
                    return;
                };
                runtime.block_on(async {
                    let mut tasks = tokio::task::JoinSet::new();
                    for (index, target, selected) in communities {
                        tasks.spawn(watch_community(
                            target,
                            index,
                            selected,
                            Arc::clone(&state),
                            watcher.clone(),
                            cancel.clone(),
                        ));
                    }
                    tasks.join_all().await;
                });
            })
            .expect("spawn community population worker");
    }
    commands.insert_resource(counts);
}

async fn watch_community(
    target: MasterTarget,
    index: usize,
    selected: bool,
    state: Arc<Mutex<Vec<Option<u32>>>>,
    mut presence: watch::Receiver<Presence>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            _ = cancel.cancelled() => return,
            wanted = presence.wait_for(|presence| presence.wanted(selected)) => {
                if wanted.is_err() {
                    return;
                }
            }
        }
        if poll_while_wanted(&target, index, selected, &state, &mut presence, &cancel)
            .await
            .is_ok()
        {
            continue;
        }
        state.lock().expect("community counts poisoned")[index] = None;
        tokio::select! {
            _ = cancel.cancelled() => return,
            _ = tokio::time::sleep(Duration::from_secs(5)) => (),
        }
    }
}

async fn poll_while_wanted(
    target: &MasterTarget,
    index: usize,
    selected: bool,
    state: &Mutex<Vec<Option<u32>>>,
    presence: &mut watch::Receiver<Presence>,
    cancel: &CancellationToken,
) -> Result<()> {
    let (_endpoint, connection) = connect(target, cancel).await?;
    let (mut send, mut recv) = cancellable(cancel, connection.open_bi()).await?;
    write_frame(
        &mut send,
        &ControlFrame::Hello(ControlHello {
            protocol_version: master_protocol::PROTOCOL_VERSION,
            game_protocol: crate::PROTOCOL_VERSION,
            role: EndpointRole::Cli,
            build: endpoint_build(),
            player_name: String::new(),
        }),
    )
    .await?;
    let mut request_id = 0;
    loop {
        let current = *presence.borrow_and_update();
        if !current.wanted(selected) {
            return Ok(());
        }
        let online = selected && current.online;
        request_id += 1;
        let players = cancellable(cancel, async {
            write_frame(
                &mut send,
                &ControlFrame::Request(ControlRequest {
                    request_id,
                    body: RequestBody::Population { online },
                }),
            )
            .await?;
            match read_frame(&mut recv).await? {
                ControlFrame::Response(ControlResponse {
                    request_id: id,
                    body: ResponseBody::Population { players },
                }) if id == request_id => Result::<u32>::Ok(players),
                frame => Err(format!("unexpected population response {frame:?}").into()),
            }
        })
        .await?;
        state.lock().expect("community counts poisoned")[index] = Some(players);
        let next = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(next);
        loop {
            tokio::select! {
                _ = cancel.cancelled() => return Ok(()),
                _ = &mut next => break,
                changed = presence.changed() => {
                    if changed.is_err() {
                        return Ok(());
                    }
                    let now = *presence.borrow();
                    if !now.wanted(selected) || (selected && now.online) != online {
                        break;
                    }
                }
            }
        }
    }
}
