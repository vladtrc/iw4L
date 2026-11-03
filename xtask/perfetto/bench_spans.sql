WITH marks AS (
  SELECT s.ts, EXTRACT_ARG(s.arg_set_id, 'debug.label') AS label,
         LEAD(s.ts) OVER (ORDER BY s.ts) AS end_ts
  FROM slice s WHERE s.name = 'benchmark_mark'
), windows AS (
  SELECT 'run' AS window, start_ts AS ts, end_ts FROM trace_bounds
  UNION ALL
  SELECT label, ts, end_ts FROM marks WHERE label GLOB '*.spawned' AND end_ts IS NOT NULL
), samples AS (
  SELECT w.window, s.name, s.dur
  FROM slice s JOIN track t ON t.id = s.track_id JOIN windows w
    ON s.ts >= w.ts AND s.ts + s.dur <= w.end_ts
  WHERE t.name GLOB 'span.*' AND s.dur >= 0
), frames AS (
  SELECT window, COUNT(*) AS frames FROM samples WHERE name = 'wall' GROUP BY window
)
SELECT s.window, s.name, COUNT(*) AS calls,
       COUNT(*) * 1.0 / f.frames AS calls_per_frame,
       SUM(s.dur) AS total_ns, AVG(s.dur) AS cost_per_call_ns,
       SUM(s.dur) * 1.0 / f.frames AS cost_per_frame_ns,
       MIN(s.dur) AS min_ns, MAX(s.dur) AS max_ns,
       PERCENTILE(s.dur, 50) AS p50_ns,
       PERCENTILE(s.dur, 95) AS p95_ns,
       PERCENTILE(s.dur, 99) AS p99_ns
FROM samples s LEFT JOIN frames f USING (window)
GROUP BY s.window, s.name ORDER BY s.window, total_ns DESC;
