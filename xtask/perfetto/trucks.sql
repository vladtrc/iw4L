-- Vehicle destructible (authority) and script-model Present gap.
SELECT
  EXTRACT_ARG(arg_set_id, 'debug.script_model_id') AS script_model_id,
  EXTRACT_ARG(arg_set_id, 'debug.state') AS state,
  EXTRACT_ARG(arg_set_id, 'debug.health') AS health,
  EXTRACT_ARG(arg_set_id, 'debug.death_clip') AS death_clip,
  EXTRACT_ARG(arg_set_id, 'debug.present_gap') AS present_gap
FROM slice
WHERE name = 'truck'
ORDER BY ts;
