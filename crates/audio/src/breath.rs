use asset_core::FamilyId;
use bevy::prelude::*;
use net::{LocalPresentClient, PresentedSnapshot};
use playerstate_iw4::{breath_hold_time_ms, weap_flags};

use crate::sources::{DesiredSource, SourceCueRequest, SourceKey};
use crate::{AliasCommand, PlayAlias};

#[derive(Resource, Default)]
pub(crate) struct BreathAudio {
    active: bool,
    holding: bool,
    selection: Option<(FamilyId, asset_game::BreathCuePolicy)>,
    heartbeat_at: i32,
}

#[derive(Resource, Default)]
pub(crate) struct BreathSources {
    pub source: Option<DesiredSource>,
    context: Option<(u32, u32)>,
    next_version: u64,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct BreathPlayer<'w> {
    bank: Option<Res<'w, crate::SoundBank>>,
    runtime: Res<'w, crate::AudioRuntime>,
    state: ResMut<'w, BreathAudio>,
    sources: ResMut<'w, BreathSources>,
    play: MessageWriter<'w, AliasCommand>,
}

impl BreathPlayer<'_> {
    fn update(
        &mut self,
        died: &mut MessageReader<frame::LifeEnded>,
        presented: Option<Res<PresentedSnapshot>>,
        local: Option<Res<LocalPresentClient>>,
        weapons: Option<Res<assets::PreparedWeapons>>,
        view: Option<Res<frame::ViewSubject>>,
        screen: Option<Res<frame::AppScreen>>,
    ) {
        let client = local.as_ref().map(|local| local.0);
        let life = client.and_then(|client| {
            presented
                .as_ref()?
                .snapshot()?
                .meta
                .for_client(client)
                .map(|meta| meta.life_sequence.0)
        });
        let context = client.zip(life).map(|(client, life)| (client.0, life));
        if self.sources.context != context {
            if self.state.active {
                stop_aliases(
                    &mut self.play,
                    self.state.selection,
                    self.sources.context.map(|(client, _)| client),
                );
            }
            self.sources.source = None;
            self.sources.context = context;
            *self.state = BreathAudio::default();
        }
        let died = died.read().any(|event| {
            client.is_some_and(|client| event.client == client.0) && life == Some(event.life)
        });
        let ps = client
            .and_then(|client| presented.as_ref()?.alive_player(client))
            .filter(|_| !view.as_ref().is_some_and(|view| view.in_killcam()));
        if died
            || !screen.is_some_and(|screen| matches!(*screen, frame::AppScreen::InGame))
            || ps.is_none()
        {
            if self.state.active {
                stop_aliases(
                    &mut self.play,
                    self.state.selection,
                    client.map(|client| client.0),
                );
            }
            self.sources.source = None;
            self.sources.context = None;
            *self.state = BreathAudio::default();
            return;
        }
        let holding = ps.is_some_and(|p| p.weap_flags & weap_flags::HOLD_BREATH != 0);
        let selection = ps.and_then(|ps| {
            let policy = weapons
                .as_ref()?
                .registry()
                .semantic_policy_of(playerstate_iw4::get_viewmodel_weapon_index(ps))?;
            Some((policy.cue_namespace, policy.breath_cues))
        });
        let Some((ns, cues)) = selection else {
            if self.state.active {
                stop_aliases(
                    &mut self.play,
                    self.state.selection,
                    client.map(|client| client.0),
                );
            }
            self.sources.source = None;
            *self.state = BreathAudio::default();
            return;
        };
        let changed = self.state.selection != selection;
        if self.state.active && changed {
            stop_aliases(
                &mut self.play,
                self.state.selection,
                client.map(|client| client.0),
            );
            self.sources.source = None;
        }
        if self.state.holding && !holding && !changed {
            self.sources.source = None;
            for alias in [cues.inhale, cues.heartbeat] {
                self.play.write(AliasCommand::Stop {
                    namespace: ns,
                    alias: alias.to_owned(),
                    snd_ent: Some(crate::SND_ENT_LOCAL),
                });
            }
            if let Some(ps) = ps {
                let alias = if ps.hold_breath_timer > breath_hold_time_ms(ps) {
                    cues.gasp
                } else {
                    cues.exhale
                };
                self.play.write(AliasCommand::Play(sound(ns, alias)));
            }
        }
        if holding && (!self.state.holding || changed) {
            self.play.write(AliasCommand::Play(sound(ns, cues.inhale)));
            self.state.heartbeat_at = ps.map_or(0, |p| p.command_time).saturating_add(1000);
        }
        if holding
            && let Some(ps) = ps
            && self.sources.source.is_none()
            && ps.command_time >= self.state.heartbeat_at
        {
            let looping = self.bank.as_ref().is_some_and(|bank| {
                bank.0.index_in(ns, cues.heartbeat).is_some_and(|index| {
                    bank.0.sound_at(index).is_some_and(|sound| {
                        (0..sound.aliases.len()).any(|variant| {
                            bank.0
                                .playback_policy(index, variant)
                                .is_some_and(|policy| policy.looping().is_looping())
                        })
                    })
                })
            });
            if looping {
                if let Some(bank) = self.bank.as_ref()
                    && let Some(cue) = self.runtime.source_cue(SourceCueRequest {
                        bank: bank.0.clone(),
                        namespace: ns,
                        alias: cues.heartbeat.into(),
                        emitter: client.map(|client| client.0),
                        scope: crate::backend::AudioScope::Match,
                        group: None,
                    })
                {
                    self.sources.next_version = self
                        .sources
                        .next_version
                        .checked_add(1)
                        .expect("source version exhausted");
                    self.sources.source = Some(DesiredSource {
                        key: SourceKey {
                            scope: crate::backend::AudioScope::Match,
                            object: u64::from(client.expect("alive client").0),
                            slot: 6,
                        },
                        version: self.sources.next_version,
                        cue,
                        origin_inches: None,
                        start_frame: self.runtime.audio_frame(),
                        gain: 1.0,
                        rate: 1.0,
                        audible: true,
                    });
                }
            } else {
                self.play
                    .write(AliasCommand::Play(sound(ns, cues.heartbeat)));
                self.state.heartbeat_at = ps.command_time.saturating_add(1000);
            }
        }
        self.state.active = true;
        self.state.holding = holding;
        self.state.selection = selection;
    }
}

pub(crate) fn update(
    presented: Option<Res<PresentedSnapshot>>,
    local: Option<Res<LocalPresentClient>>,
    weapons: Option<Res<assets::PreparedWeapons>>,
    view: Option<Res<frame::ViewSubject>>,
    screen: Option<Res<frame::AppScreen>>,
    mut died: MessageReader<frame::LifeEnded>,
    mut player: BreathPlayer,
) {
    player.update(&mut died, presented, local, weapons, view, screen);
}

fn stop_aliases(
    play: &mut MessageWriter<AliasCommand>,
    selection: Option<(FamilyId, asset_game::BreathCuePolicy)>,
    client: Option<u32>,
) {
    let Some((namespace, cues)) = selection else {
        return;
    };
    for alias in cues.aliases() {
        play.write(AliasCommand::Stop {
            namespace,
            alias: alias.to_owned(),
            snd_ent: client,
        });
    }
}

fn sound(namespace: FamilyId, alias: &str) -> PlayAlias {
    PlayAlias {
        event: None,
        namespace,
        alias: alias.to_owned(),
        fallback: None,
        origin_inches: None,
        snd_ent: Some(crate::SND_ENT_LOCAL),
    }
}
