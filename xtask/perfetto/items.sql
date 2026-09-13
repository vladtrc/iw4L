-- Dropped ET_ITEM (authority) and Present posed worldModel.
SELECT
  EXTRACT_ARG(arg_set_id, 'debug.e_type') AS e_type,
  EXTRACT_ARG(arg_set_id, 'debug.clip_r') AS clip_r,
  EXTRACT_ARG(arg_set_id, 'debug.scavenger') AS scavenger,
  EXTRACT_ARG(arg_set_id, 'debug.present_gap') AS present_gap
FROM slice
WHERE name = 'item'
ORDER BY ts;
