-- Item pickups granted this tick.
SELECT
  EXTRACT_ARG(arg_set_id, 'debug.picker_pm_type') AS picker_pm_type
FROM slice
WHERE name = 'pickup'
ORDER BY ts;
