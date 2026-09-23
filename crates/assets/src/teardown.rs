use bevy::prelude::*;
use frame::{ClientSet, MatchTornDown, ReturnedToMenu, SessionSwapApplied};

use crate::{
    AssetRefDumpCensus, MapXModelSceneCatalog, MatchType10SoundHints, PlayerAnimSources,
    PreparedBodies, PreparedBodyClips, PreparedDestructibleDeath, PreparedFpvMeshes,
    PreparedLocalizedStrings, PreparedProjectileMeshes, PreparedWeapons, PreparedWorldWeapons,
    PreparedXAnims, PreparedXModelWalkCensus, SessionCompass, SessionMapScriptSound,
    SessionTeamSettings,
};

pub(crate) fn drop_match_catalogs_on_teardown(
    mut torn: MessageReader<MatchTornDown>,
    mut returned: MessageReader<ReturnedToMenu>,
    mut commands: Commands,
) {
    if torn.read().count() == 0 && returned.read().count() == 0 {
        return;
    }
    commands.queue(retire_match_catalogs);
}

fn retire_match_catalogs(world: &mut World) {
    frame::retire::retire_resources(world, |batch| {
        batch
            .resource::<PreparedWeapons>()
            .resource::<MatchType10SoundHints>()
            .resource::<PreparedFpvMeshes>()
            .resource::<PreparedBodies>()
            .resource::<PreparedWorldWeapons>()
            .resource::<PreparedProjectileMeshes>()
            .resource::<PreparedXModelWalkCensus>()
            .resource::<PreparedXAnims>()
            .resource::<PlayerAnimSources>()
            .resource::<SessionCompass>()
            .resource::<SessionMapScriptSound>()
            .resource::<SessionTeamSettings>()
            .resource::<PreparedLocalizedStrings>()
            .resource::<PreparedDestructibleDeath>()
            .resource::<PreparedBodyClips>()
            .resource::<MapXModelSceneCatalog>()
            .resource::<AssetRefDumpCensus>();
    });
}

pub(crate) fn register_match_teardown(app: &mut App) {
    app.add_message::<ReturnedToMenu>().add_systems(
        Update,
        drop_match_catalogs_on_teardown
            .after(SessionSwapApplied)
            .in_set(ClientSet::Load),
    );
}
