-- Corpse slots (authority origin) and Present DEATH leaf names.
SELECT
  EXTRACT_ARG(arg_set_id, 'debug.occupied') AS occupied,
  EXTRACT_ARG(arg_set_id, 'debug.origin_z') AS origin_z,
  EXTRACT_ARG(arg_set_id, 'debug.pose_e_type') AS pose_e_type,
  EXTRACT_ARG(arg_set_id, 'debug.legs_leaf_name') AS legs_leaf_name
FROM slice
WHERE name = 'corpse'
ORDER BY ts;
