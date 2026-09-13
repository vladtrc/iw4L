SELECT
  ct.name,
  COUNT(*) AS n,
  AVG(c.value) AS avg_value,
  MAX(c.value) AS max_value
FROM counter c
JOIN counter_track ct ON c.track_id = ct.id
GROUP BY ct.name
ORDER BY n DESC;
