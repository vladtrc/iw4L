use asset_core::FamilyId;
use bevy::prelude::*;
use frame::{AppScreen, LifeEnded};
use hud_iw4::shellshock_remaining_ms;
use net::{FrameClock, LocalPresentClient, PresentedSnapshot};

use crate::sources::{DesiredSource, SourceCueRequest, SourceKey};
use crate::{PlayAlias, SND_ENT_LOCAL, playback::SoundBank};

#[derive(Resource, Default)]
pub(crate) struct ShellshockSources {
    pub source: Option<DesiredSource>,
    context: Option<(u32, u32)>,
    next_version: u64,
    was_active: bool,
}

#[derive(bevy::ecs::system::SystemParam)]
pub(crate) struct ShellshockPlayer<'w> {
    bank: Option<Res<'w, SoundBank>>,
    runtime: Res<'w, crate::AudioRuntime>,
    sources: ResMut<'w, ShellshockSources>,
    play: MessageWriter<'w, crate::AliasCommand>,
}

impl ShellshockPlayer<'_> {
    fn update(
        &mut self,
        died: &mut MessageReader<LifeEnded>,
        screen: Option<Res<AppScreen>>,
        cg_clock: Option<Res<FrameClock>>,
        presented: Option<Res<PresentedSnapshot>>,
        local: Option<Res<LocalPresentClient>>,
    ) {
        let local_id = local.as_ref().map(|l| l.0.0);
        let life = local.as_ref().and_then(|local| {
            presented
                .as_ref()?
                .snapshot()?
                .meta
                .for_client(local.0)
                .map(|meta| meta.life_sequence.0)
        });
        let died = died
            .read()
            .any(|ev| local_id == Some(ev.client) && life == Some(ev.life));
        if !screen.is_some_and(|s| matches!(*s, AppScreen::InGame)) {
            self.sources.source = None;
            self.sources.context = None;
            self.sources.was_active = false;
            return;
        }
        let (Some(cg_clock), Some(presented), Some(local)) = (cg_clock, presented, local) else {
            self.sources.source = None;
            self.sources.context = None;
            self.sources.was_active = false;
            return;
        };
        let context = (local.0.0, life.unwrap_or(0));
        if self.sources.context != Some(context) {
            self.sources.source = None;
            self.sources.was_active = false;
            self.sources.context = Some(context);
        }
        let alive = presented.alive_player(local.0).is_some();
        let remaining = presented
            .player(local.0)
            .map(|ps| {
                shellshock_remaining_ms(cg_clock.time(), ps.shellshock_time, ps.shellshock_duration)
            })
            .unwrap_or(0);
        let Some(parms) = presented
            .shellshock(local.0)
            .map(|shock| shock.sound.clone())
        else {
            self.sources.source = None;
            self.sources.was_active = false;
            return;
        };
        let want = remaining > 0 && parms.affect && alive && !died;
        if want {
            let same = self
                .sources
                .source
                .as_ref()
                .is_some_and(|source| source.cue.alias == parms.loop_alias);
            if !same {
                self.sources.source = None;
                if let Some(bank) = self.bank.as_ref()
                    && let Some(cue) = self.runtime.source_cue(SourceCueRequest {
                        bank: bank.0.clone(),
                        namespace: FamilyId::IW4,
                        alias: parms.loop_alias.clone(),
                        emitter: Some(local.0.0),
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
                            object: u64::from(local.0.0),
                            slot: 5,
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
            }
            self.sources.was_active = true;
            return;
        }
        if self.sources.was_active {
            self.sources.source = None;
            let alias = if died || !alive {
                &parms.abort_alias
            } else {
                &parms.end_alias
            };
            self.play.write(crate::AliasCommand::Play(PlayAlias {
                event: None,
                namespace: FamilyId::IW4,
                alias: alias.to_owned(),
                fallback: None,
                origin_inches: None,
                snd_ent: Some(SND_ENT_LOCAL),
            }));
        }
        self.sources.was_active = false;
    }
}

pub(crate) fn update_shellshock_tinnitus(
    mut died: MessageReader<LifeEnded>,
    screen: Option<Res<AppScreen>>,
    cg_clock: Option<Res<FrameClock>>,
    presented: Option<Res<PresentedSnapshot>>,
    local: Option<Res<LocalPresentClient>>,
    mut player: ShellshockPlayer,
) {
    player.update(&mut died, screen, cg_clock, presented, local);
}
