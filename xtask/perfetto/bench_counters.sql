WITH marks AS (
  SELECT s.ts, EXTRACT_ARG(s.arg_set_id, 'debug.label') AS label,
         LEAD(s.ts) OVER (ORDER BY s.ts) AS end_ts
  FROM slice s WHERE s.name = 'benchmark_mark'
), windows AS (
  SELECT 'run' AS window, start_ts AS ts, end_ts FROM trace_bounds
  UNION ALL
  SELECT label, ts, end_ts FROM marks WHERE label GLOB '*.spawned' AND end_ts IS NOT NULL
), frames AS (
  SELECT w.window, COUNT(*) AS frames FROM slice s JOIN windows w
    ON s.ts >= w.ts AND s.ts + s.dur <= w.end_ts
  WHERE s.name = 'wall' AND s.dur >= 0 GROUP BY w.window
)
SELECT w.window, t.name, COUNT(*) AS samples,
       COUNT(*) * 1.0 / f.frames AS samples_per_frame,
       SUM(c.value) AS total, AVG(c.value) AS per_call,
       SUM(c.value) / f.frames AS per_frame,
       MIN(c.value) AS min, MAX(c.value) AS max,
       PERCENTILE(c.value, 50) AS p50,
       PERCENTILE(c.value, 95) AS p95,
       PERCENTILE(c.value, 99) AS p99
FROM counter c JOIN counter_track t ON t.id = c.track_id
JOIN windows w ON c.ts >= w.ts AND c.ts < w.end_ts
LEFT JOIN frames f USING (window)
GROUP BY w.window, t.name ORDER BY w.window, t.name;
