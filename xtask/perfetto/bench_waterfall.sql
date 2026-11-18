WITH states AS (
  SELECT ts, EXTRACT_ARG(arg_set_id, 'debug.loading') AS loading,
    EXTRACT_ARG(arg_set_id, 'debug.spawned') AS spawned,
    EXTRACT_ARG(arg_set_id, 'debug.ingame') AS ingame,
    EXTRACT_ARG(arg_set_id, 'debug.command_elapsed_ns') AS command_elapsed_ns
  FROM slice WHERE name = 'load_state'
), points AS (
  SELECT 'command' AS milestone, MIN(ts - command_elapsed_ns) AS ts FROM states
  UNION ALL SELECT 'loading observed', MIN(ts) FROM states WHERE loading = 1
  UNION ALL SELECT 'installed', MIN(ts) FROM slice WHERE name = 'match_installed'
  UNION ALL SELECT 'spawned', MIN(ts) FROM states WHERE spawned = 1
  UNION ALL SELECT 'overlay down', MIN(ts) FROM states WHERE loading = 0
    AND ts > (SELECT MIN(ts) FROM states WHERE loading = 1)
  UNION ALL SELECT 'InGame', MIN(ts) FROM states WHERE ingame = 1
  UNION ALL SELECT 'playable', MIN(ts) FROM slice WHERE name = 'benchmark_playable'
  UNION ALL SELECT 'ambient', MIN(ts) FROM slice WHERE name = 'ambient_boot'
  UNION ALL SELECT 'screenshot', MIN(ts) FROM slice WHERE name = 'screenshot'
), origin AS (
  SELECT COALESCE(MIN(CASE WHEN milestone = 'command' THEN ts END), MIN(ts)) AS ts FROM points
)
SELECT milestone, p.ts, (p.ts - o.ts) / 1e6 AS elapsed_ms,
  (p.ts - LAG(p.ts) OVER (ORDER BY p.ts)) / 1e6 AS since_previous_ms
FROM points p, origin o WHERE p.ts IS NOT NULL ORDER BY p.ts;
