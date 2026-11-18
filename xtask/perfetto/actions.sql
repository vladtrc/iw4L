WITH expected(name) AS (
  VALUES ('client_actions_sent'), ('client_actions_processed'),
         ('client_actions_applied'), ('client_actions_refused')
), counts AS (
  SELECT t.name, COUNT(*) AS samples, SUM(c.value) AS total
  FROM counter c JOIN counter_track t ON t.id = c.track_id
  WHERE t.name IN (SELECT name FROM expected)
  GROUP BY t.name
)
SELECT e.name, COALESCE(c.samples, 0) AS samples,
       CAST(COALESCE(c.total, 0) AS INT) AS total
FROM expected e LEFT JOIN counts c USING (name) ORDER BY e.name;
