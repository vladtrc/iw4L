WITH stages AS (
  SELECT id, EXTRACT_ARG(arg_set_id, 'debug.label') AS label,
         EXTRACT_ARG(arg_set_id, 'debug.start_ns') AS start_ns,
         EXTRACT_ARG(arg_set_id, 'debug.duration_ns') AS duration_ns,
         EXTRACT_ARG(arg_set_id, 'debug.done') AS done,
         EXTRACT_ARG(arg_set_id, 'debug.total') AS total,
         EXTRACT_ARG(arg_set_id, 'debug.rss_delta') AS rss_delta,
         EXTRACT_ARG(arg_set_id, 'debug.heap_delta') AS heap_delta,
         EXTRACT_ARG(arg_set_id, 'debug.outcome') AS outcome
  FROM slice WHERE name = 'load_stage'
), points AS (
  SELECT start_ns AS t FROM stages UNION
  SELECT start_ns + duration_ns FROM stages
), intervals AS (
  SELECT t, LEAD(t) OVER (ORDER BY t) AS end_t FROM points
), occupied AS (
  SELECT i.t, i.end_t, COUNT(*) AS active, MIN(s.id) AS only_stage
  FROM intervals i JOIN stages s
    ON s.start_ns < i.end_t AND s.start_ns + s.duration_ns > i.t
  GROUP BY i.t, i.end_t
)
SELECT s.label, s.start_ns / 1e6 AS start_ms, s.duration_ns / 1e6 AS duration_ms,
       COALESCE(SUM(o.end_t - o.t), 0) / 1e6 AS sole_open_ms, s.done, s.total, s.rss_delta, s.heap_delta, s.outcome
FROM stages s LEFT JOIN occupied o ON o.active = 1 AND o.only_stage = s.id
GROUP BY s.id ORDER BY s.start_ns, s.id;
