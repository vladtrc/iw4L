CREATE PERFETTO TABLE bench_wall AS
SELECT id AS frame, ts, dur FROM slice WHERE name = 'wall' AND dur > 0;

CREATE PERFETTO TABLE bench_roots AS
WITH roots AS (
  SELECT ts, ts + dur AS end_ts FROM slice
  WHERE name IN ('PreUpdate', 'FixedUpdate', 'Update', 'PostUpdate', 'render_thread')
    AND dur > 0
), previous AS (
  SELECT *, MAX(end_ts) OVER (
    ORDER BY ts, end_ts ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING
  ) AS covered_until FROM roots
), groups AS (
  SELECT *, SUM(CASE WHEN covered_until IS NULL OR ts > covered_until THEN 1 ELSE 0 END)
    OVER (ORDER BY ts, end_ts) AS segment FROM previous
)
SELECT MIN(ts) AS ts, MAX(end_ts) - MIN(ts) AS dur FROM groups GROUP BY segment;

CREATE VIRTUAL TABLE bench_covered USING SPAN_JOIN(bench_wall, bench_roots);

WITH coverage AS (
  SELECT frame, SUM(dur) AS covered_ns FROM bench_covered GROUP BY frame
)
SELECT w.frame, w.ts, w.dur AS wall_ns, COALESCE(c.covered_ns, 0) AS covered_ns,
       w.dur - COALESCE(c.covered_ns, 0) AS unclassified_ns
FROM bench_wall w LEFT JOIN coverage c USING (frame) ORDER BY w.ts;
