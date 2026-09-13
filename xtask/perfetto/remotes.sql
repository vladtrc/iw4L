-- Remote ET_PLAYER centity pose and proxy starve.
-- origin_* is CEntityRuntime; snap_origin_* is the adopted nextSnap row Apply sampled.
SELECT
  EXTRACT_ARG(arg_set_id, 'debug.client') AS client,
  EXTRACT_ARG(arg_set_id, 'debug.time_ms') AS time_ms,
  EXTRACT_ARG(arg_set_id, 'debug.pose_e_type') AS pose_e_type,
  EXTRACT_ARG(arg_set_id, 'debug.origin_x') AS origin_x,
  EXTRACT_ARG(arg_set_id, 'debug.origin_y') AS origin_y,
  EXTRACT_ARG(arg_set_id, 'debug.origin_z') AS origin_z,
  EXTRACT_ARG(arg_set_id, 'debug.snap_origin_x') AS snap_origin_x,
  EXTRACT_ARG(arg_set_id, 'debug.snap_origin_y') AS snap_origin_y,
  EXTRACT_ARG(arg_set_id, 'debug.snap_origin_z') AS snap_origin_z,
  EXTRACT_ARG(arg_set_id, 'debug.proxy_outcome') AS proxy_outcome
FROM slice
WHERE name = 'remote'
ORDER BY ts;
