use bevy::prelude::*;
use frame::{ClientSet, MatchTornDown, SessionSwapApplied};

use crate::{
    AssetRefDumpCensus, MapXModelSceneCatalog, MatchType10SoundHints, PlayerAnimSources,
    PreparedBodies, PreparedBodyClips, PreparedDestructibleDeath, PreparedFpvMeshes,
    PreparedLocalizedStrings, PreparedProjectileMeshes, PreparedWeapons, PreparedWorldWeapons,
    PreparedXAnims, PreparedXModelWalkCensus, SessionCompass, SessionMapScriptSound,
    SessionTeamIcons,
};

pub(crate) fn drop_match_catalogs_on_teardown(
    mut torn: MessageReader<MatchTornDown>,
    mut commands: Commands,
) {
    if torn.read().count() == 0 {
        return;
    }
    commands.remove_resource::<PreparedWeapons>();
    commands.remove_resource::<MatchType10SoundHints>();
    commands.remove_resource::<PreparedFpvMeshes>();
    commands.remove_resource::<PreparedBodies>();
    commands.remove_resource::<PreparedWorldWeapons>();
    commands.remove_resource::<PreparedProjectileMeshes>();
    commands.remove_resource::<PreparedXModelWalkCensus>();
    commands.remove_resource::<PreparedXAnims>();
    commands.remove_resource::<PlayerAnimSources>();
    commands.remove_resource::<SessionCompass>();
    commands.remove_resource::<SessionMapScriptSound>();
    commands.remove_resource::<SessionTeamIcons>();
    commands.remove_resource::<PreparedLocalizedStrings>();
    commands.remove_resource::<PreparedDestructibleDeath>();
    commands.remove_resource::<PreparedBodyClips>();
    commands.remove_resource::<MapXModelSceneCatalog>();
    commands.remove_resource::<AssetRefDumpCensus>();
}

pub(crate) fn register_match_teardown(app: &mut App) {
    app.add_systems(
        Update,
        drop_match_catalogs_on_teardown
            .after(SessionSwapApplied)
            .in_set(ClientSet::Load),
    );
}
