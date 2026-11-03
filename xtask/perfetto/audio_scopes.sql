-- Instance IDs are existing mixer voice identities, not presentation epochs.
WITH created AS (
  SELECT ts, EXTRACT_ARG(arg_set_id, 'debug.instance') AS instance,
         EXTRACT_ARG(arg_set_id, 'debug.scope') AS scope
  FROM slice WHERE name = 'audio_instance_created'
), started AS (
  SELECT ts, EXTRACT_ARG(arg_set_id, 'debug.instance') AS instance,
         EXTRACT_ARG(arg_set_id, 'debug.scope') AS scope
  FROM slice WHERE name = 'audio_instance_started'
), stopped AS (
  SELECT ts, EXTRACT_ARG(arg_set_id, 'debug.scope') AS scope
  FROM slice WHERE name = 'audio_scope_stop'
), cancelled AS (
  SELECT ts, EXTRACT_ARG(arg_set_id, 'debug.instance') AS instance
  FROM slice WHERE name = 'audio_scope_cancel'
)
SELECT 'scope_stops' AS metric, COUNT(*) AS total FROM stopped
UNION ALL SELECT 'cancelled_instances', COUNT(*) FROM cancelled
UNION ALL SELECT 'match_starts', COUNT(*) FROM started WHERE scope = 1
UNION ALL
SELECT 'old_instance_starts_after_stop', COUNT(DISTINCT started.instance)
FROM started JOIN created USING(instance)
WHERE started.scope = 1 AND EXISTS (
  SELECT 1 FROM stopped WHERE stopped.scope = 1
    AND created.ts < stopped.ts AND started.ts > stopped.ts
)
UNION ALL
SELECT 'cancelled_instance_starts', COUNT(DISTINCT started.instance)
FROM started JOIN cancelled USING(instance) WHERE started.ts > cancelled.ts;
