-- All duration slices. Agents start here; order is total time, not name.
SELECT
  name,
  COUNT(*) AS n,
  CAST(AVG(dur) / 1e6 AS REAL) AS avg_ms,
  CAST(MAX(dur) / 1e6 AS REAL) AS max_ms,
  CAST(SUM(dur) / 1e6 AS REAL) AS sum_ms
FROM slice
GROUP BY name
ORDER BY sum_ms DESC;
