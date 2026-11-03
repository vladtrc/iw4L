use bevy::prelude::*;

use super::ConsoleEcho;
use crate::ConsoleCommand;

const MAX_BURST: usize = 8192;

pub(crate) fn route_sound_commands(
    mut commands: MessageReader<ConsoleCommand>,
    bank: Option<Res<audio::SoundBank>>,
    mut aliases: MessageWriter<audio::AliasCommand>,
    mut echo: ConsoleEcho,
) {
    for command in commands.read().filter(|command| command.name == "sound") {
        let [key, tail @ ..] = command.args.as_slice() else {
            echo.write("usage: sound [family:]alias [count]");
            continue;
        };
        let count = match tail {
            [] => 1,
            [count] => match count.parse::<usize>() {
                Ok(count) if (1..=MAX_BURST).contains(&count) => count,
                _ => {
                    echo.write(format!("sound: count must be 1..={MAX_BURST}"));
                    continue;
                }
            },
            _ => {
                echo.write("usage: sound [family:]alias [count]");
                continue;
            }
        };
        let Some(bank) = bank.as_ref() else {
            echo.write("sound: match sound bank is unavailable");
            continue;
        };
        let index = match key.split_once(':') {
            Some((family, alias)) => asset_core::FamilyId::parse(family)
                .and_then(|family| bank.0.index_in(family, alias)),
            None => bank.0.index_unique(key),
        };
        let Some(sound) = index.and_then(|index| bank.0.sound_at(index)) else {
            echo.write(format!("sound: alias is missing or ambiguous: {key}"));
            continue;
        };
        for _ in 0..count {
            aliases.write(audio::AliasCommand::Play(audio::PlayAlias {
                event: None,
                namespace: sound.game,
                alias: sound.name.clone(),
                fallback: None,
                origin_inches: None,
                snd_ent: Some(audio::SND_ENT_LOCAL),
            }));
        }
        echo.write(format!(
            "sound: queued {count} requests for {}:{}",
            sound.game.as_str(),
            sound.name
        ));
    }
}
