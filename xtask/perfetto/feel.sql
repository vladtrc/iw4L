-- Killcam seat / clock debt / origin split.
SELECT
  EXTRACT_ARG(arg_set_id, 'debug.time_ms') AS time_ms,
  EXTRACT_ARG(arg_set_id, 'debug.lifecycle') AS lifecycle,
  EXTRACT_ARG(arg_set_id, 'debug.fanout_seat_applied') AS fanout_seat_applied,
  EXTRACT_ARG(arg_set_id, 'debug.seat_lookup_tick') AS seat_lookup_tick,
  EXTRACT_ARG(arg_set_id, 'debug.present_choice') AS present_choice,
  EXTRACT_ARG(arg_set_id, 'debug.present_snapshot_delta_time') AS present_snapshot_delta_time,
  EXTRACT_ARG(arg_set_id, 'debug.authority_origin_x') AS authority_origin_x,
  EXTRACT_ARG(arg_set_id, 'debug.authority_origin_y') AS authority_origin_y,
  EXTRACT_ARG(arg_set_id, 'debug.authority_origin_z') AS authority_origin_z,
  EXTRACT_ARG(arg_set_id, 'debug.adopted_origin_x') AS adopted_origin_x,
  EXTRACT_ARG(arg_set_id, 'debug.adopted_origin_y') AS adopted_origin_y,
  EXTRACT_ARG(arg_set_id, 'debug.adopted_origin_z') AS adopted_origin_z,
  EXTRACT_ARG(arg_set_id, 'debug.client_clock_debt_ms') AS client_clock_debt_ms,
  EXTRACT_ARG(arg_set_id, 'debug.ingress_queue_depth') AS ingress_queue_depth,
  EXTRACT_ARG(arg_set_id, 'debug.authority_command_time') AS authority_command_time,
  EXTRACT_ARG(arg_set_id, 'debug.predicted_command_time') AS predicted_command_time
FROM slice
WHERE name = 'feel'
ORDER BY ts;
