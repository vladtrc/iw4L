-- Render topic pack: existing producer durations and counters, active gameplay only.
WITH active AS (
  SELECT COALESCE(MIN(ts), (SELECT MIN(ts) FROM slice)) AS start
  FROM slice
  WHERE name = 'player_tick'
),
render_spans AS (
  SELECT
    'span' AS kind,
    name,
    COUNT(*) AS n,
    ROUND(AVG(dur) / 1000000.0, 3) AS avg_value,
    ROUND(MAX(dur) / 1000000.0, 3) AS max_value,
    'ms' AS unit
  FROM slice, active
  WHERE ts >= active.start
    AND dur >= 0
    AND name IN (
      'wall', 'PostUpdate', 'Present', 'extract_wait', 'render_thread',
      'render_render', 'post_execute', 'post_rebuild', 'colour_submit',
      'skin_model', 'cull'
    )
  GROUP BY name
),
render_counters AS (
  SELECT
    'counter' AS kind,
    ct.name,
    COUNT(*) AS n,
    ROUND(AVG(c.value), 3) AS avg_value,
    ROUND(MAX(c.value), 3) AS max_value,
    CASE WHEN ct.name = 'gpu_frame' THEN 'ms' ELSE 'count' END AS unit
  FROM counter c
  JOIN counter_track ct ON c.track_id = ct.id, active
  WHERE c.ts >= active.start
    AND ct.name IN ('draws', 'batches', 'gpu_frame')
  GROUP BY ct.name
)
SELECT * FROM render_spans
UNION ALL
SELECT * FROM render_counters
ORDER BY kind DESC, name;
