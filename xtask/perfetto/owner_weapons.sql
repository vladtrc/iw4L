WITH delivered AS (
 SELECT EXTRACT_ARG(arg_set_id,'debug.kind') kind,
 EXTRACT_ARG(arg_set_id,'debug.domain') domain,
 EXTRACT_ARG(arg_set_id,'debug.number') number,
 EXTRACT_ARG(arg_set_id,'debug.local') local,
 EXTRACT_ARG(arg_set_id,'debug.archived') archived,
 EXTRACT_ARG(arg_set_id,'debug.world_hit') world_hit
 FROM slice WHERE name='weapon_fx_event'
), producers AS (
 SELECT EXTRACT_ARG(arg_set_id,'debug.entity') entity,
 EXTRACT_ARG(arg_set_id,'debug.sequence') sequence,
 EXTRACT_ARG(arg_set_id,'debug.tick') tick,
 EXTRACT_ARG(arg_set_id,'debug.domain') domain,
 EXTRACT_ARG(arg_set_id,'debug.alias') alias,
 EXTRACT_ARG(arg_set_id,'debug.player_view') player_view
 FROM slice WHERE name='weapon_fire_audio'
), decisions AS (
 SELECT EXTRACT_ARG(arg_set_id,'debug.entity') entity,
 EXTRACT_ARG(arg_set_id,'debug.sequence') sequence,
 EXTRACT_ARG(arg_set_id,'debug.tick') tick,
 EXTRACT_ARG(arg_set_id,'debug.domain') domain,
 EXTRACT_ARG(arg_set_id,'debug.alias') alias,
 EXTRACT_ARG(arg_set_id,'debug.ordinal') ordinal,
 EXTRACT_ARG(arg_set_id,'debug.submitted') submitted
 FROM slice WHERE name='audio_start_decision'
), started AS (
 SELECT p.domain,p.player_view FROM producers p JOIN decisions d
 USING(entity,sequence,tick,domain,alias) WHERE d.submitted=1 AND d.ordinal=0
)
SELECT 'predicted_fires' metric,COUNT(*) value FROM slice WHERE name='predicted_weapon_fire'
UNION ALL SELECT 'owner_direct_fires',COUNT(*) FROM delivered WHERE kind='fire' AND number=local AND domain='predicted'
UNION ALL SELECT 'owner_replica_fires',COUNT(*) FROM delivered WHERE kind='fire' AND number=local AND domain<>'predicted' AND archived=0
UNION ALL SELECT 'owner_replica_brass',COUNT(*) FROM delivered WHERE kind='brass' AND number=local AND domain<>'predicted' AND archived=0
UNION ALL SELECT 'owner_replica_world_hits',COUNT(*) FROM delivered WHERE kind='hit' AND number=local AND domain<>'predicted' AND archived=0 AND world_hit=1
UNION ALL SELECT 'owner_audio_producers',COUNT(*) FROM producers WHERE domain='predicted'
UNION ALL SELECT 'owner_audio_submitted',COUNT(*) FROM started WHERE domain='predicted'
UNION ALL SELECT 'killcam_audio_producers',COUNT(*) FROM producers WHERE domain='snapshot' AND player_view=1
UNION ALL SELECT 'killcam_audio_submitted',COUNT(*) FROM started WHERE domain='snapshot' AND player_view=1;
