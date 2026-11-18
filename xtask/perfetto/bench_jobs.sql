WITH jobs AS (
  SELECT EXTRACT_ARG(arg_set_id, 'debug.data') AS data FROM slice WHERE name = 'load_job'
)
SELECT JSON_EXTRACT(data, '$.id') AS id,
       JSON_EXTRACT(data, '$.kind') AS kind,
       JSON_EXTRACT(data, '$.canonical') AS canonical,
       (JSON_EXTRACT(data, '$.enqueued_ns') - JSON_EXTRACT(data, '$.plan_ready_ns')) / 1e6 AS ready_to_queue_ms,
       (JSON_EXTRACT(data, '$.started_ns') - JSON_EXTRACT(data, '$.enqueued_ns')) / 1e6 AS queue_ms,
       (JSON_EXTRACT(data, '$.decode_started_ns') - JSON_EXTRACT(data, '$.started_ns')) / 1e6 AS budget_ms,
       (JSON_EXTRACT(data, '$.finished_ns') - JSON_EXTRACT(data, '$.decode_started_ns')) / 1e6 AS decode_ms,
       (JSON_EXTRACT(data, '$.joined_ns') - JSON_EXTRACT(data, '$.finished_ns')) / 1e6 AS done_to_join_ms,
       JSON_EXTRACT(data, '$.prepared_bytes') AS prepared_bytes,
       JSON_EXTRACT(data, '$.reused_bytes') AS reused_bytes,
       JSON_EXTRACT(data, '$.retained_bytes') AS retained_bytes,
       JSON_EXTRACT(data, '$.discarded_bytes') AS discarded_bytes
FROM jobs ORDER BY id;
