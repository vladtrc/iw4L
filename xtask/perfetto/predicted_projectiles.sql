WITH ev AS (
 SELECT name,
 EXTRACT_ARG(arg_set_id,'debug.client') client,
 EXTRACT_ARG(arg_set_id,'debug.life') life,
 EXTRACT_ARG(arg_set_id,'debug.command') command,
 EXTRACT_ARG(arg_set_id,'debug.ordinal') ordinal,
 EXTRACT_ARG(arg_set_id,'debug.hand') hand,
 ts
 FROM slice WHERE name IN ('predicted_projectile_spawn','predicted_projectile_matched','predicted_projectile_timed_out')
), spawn AS (SELECT * FROM ev WHERE name='predicted_projectile_spawn'),
matched AS (SELECT * FROM ev WHERE name='predicted_projectile_matched'),
timed AS (SELECT * FROM ev WHERE name='predicted_projectile_timed_out')
SELECT 'spawned' metric, COUNT(*) value FROM spawn
UNION ALL SELECT 'matched', COUNT(*) FROM matched
UNION ALL SELECT 'timed_out', COUNT(*) FROM timed
UNION ALL SELECT 'matched_to_spawn', COUNT(*) FROM matched m JOIN spawn s USING(client,life,command,ordinal,hand)
UNION ALL SELECT 'matched_and_timed_out', COUNT(*) FROM matched m JOIN timed t USING(client,life,command,ordinal,hand)
UNION ALL SELECT 'match_latency_ms_max', CAST(MAX(m.ts-s.ts)/1e6 AS INT) FROM matched m JOIN spawn s USING(client,life,command,ordinal,hand)
UNION ALL SELECT 'match_latency_ms_avg', CAST(AVG(m.ts-s.ts)/1e6 AS INT) FROM matched m JOIN spawn s USING(client,life,command,ordinal,hand)
UNION ALL SELECT 'authority_projectiles', COUNT(*) FROM slice WHERE name='projectile'
