WITH intakes AS (
  SELECT ts, EXTRACT_ARG(arg_set_id, 'debug.object') AS object,
         EXTRACT_ARG(arg_set_id, 'debug.slot') AS slot,
         EXTRACT_ARG(arg_set_id, 'debug.scope') AS scope,
         EXTRACT_ARG(arg_set_id, 'debug.alias') AS alias
  FROM slice WHERE name = 'audio_source_intake'
), stops AS (
  SELECT ts, EXTRACT_ARG(arg_set_id, 'debug.object') AS object,
         EXTRACT_ARG(arg_set_id, 'debug.slot') AS slot,
         EXTRACT_ARG(arg_set_id, 'debug.scope') AS scope
  FROM slice WHERE name = 'audio_source_stop'
), scope_stops AS (
  SELECT ts FROM slice WHERE name = 'audio_scope_stop'
    AND EXTRACT_ARG(arg_set_id, 'debug.scope') = 1
), rockets AS (
  SELECT object, alias,
         COALESCE((SELECT MAX(ts) FROM scope_stops WHERE ts < i.ts), 0) AS timeline,
         COUNT(*) AS starts
  FROM intakes i WHERE scope = 1 AND slot = 7
  GROUP BY object, alias, timeline
), created AS (
  SELECT ts, EXTRACT_ARG(arg_set_id, 'debug.instance') AS instance,
         EXTRACT_ARG(arg_set_id, 'debug.source_object') AS object,
         EXTRACT_ARG(arg_set_id, 'debug.source_slot') AS slot,
         EXTRACT_ARG(arg_set_id, 'debug.scope') AS scope
  FROM slice WHERE name = 'audio_instance_created'
), started AS (
  SELECT ts, EXTRACT_ARG(arg_set_id, 'debug.instance') AS instance
  FROM slice WHERE name = 'audio_instance_started'
), loop_counts AS (
  SELECT c.ts, c.value FROM counter c JOIN counter_track t ON t.id = c.track_id
  WHERE t.name = 'match_loop_sources'
), torn AS (
  SELECT ts FROM slice WHERE name = 'benchmark_mark'
    AND EXTRACT_ARG(arg_set_id, 'debug.label') = 'sources_torn'
)
SELECT 'rocket_sources' AS metric, COUNT(*) AS total FROM rockets
UNION ALL SELECT 'rocket_source_restarts', COALESCE(SUM(starts - 1), 0) FROM rockets
UNION ALL SELECT 'source_stops', COUNT(*) FROM stops
UNION ALL SELECT 'stopped_source_instances', COUNT(DISTINCT c.instance)
FROM created c WHERE EXISTS (SELECT 1 FROM stops x WHERE x.object = c.object
  AND x.slot = c.slot AND x.scope = c.scope AND c.ts < x.ts)
UNION ALL SELECT 'old_source_starts_after_stop', COUNT(DISTINCT c.instance)
FROM created c JOIN started s USING(instance)
WHERE EXISTS (SELECT 1 FROM stops x WHERE x.object = c.object AND x.slot = c.slot
  AND x.scope = c.scope AND c.ts < x.ts AND s.ts > x.ts)
UNION ALL SELECT 'peak_match_loop_sources', COALESCE(MAX(value), -1) FROM loop_counts
UNION ALL SELECT 'final_match_loop_sources', COALESCE((SELECT value FROM loop_counts ORDER BY ts DESC LIMIT 1), -1)
UNION ALL SELECT 'sources_torn_marks', COUNT(*) FROM torn
UNION ALL SELECT 'match_loop_sources_at_torn', COALESCE((
  SELECT value FROM loop_counts WHERE ts <= (SELECT MAX(ts) FROM torn) ORDER BY ts DESC LIMIT 1
), -1);
