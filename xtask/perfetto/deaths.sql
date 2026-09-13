-- SimEvent::Died at the journal push.
SELECT
  EXTRACT_ARG(arg_set_id, 'debug.victim') AS victim,
  EXTRACT_ARG(arg_set_id, 'debug.attacker') AS attacker,
  EXTRACT_ARG(arg_set_id, 'debug.suicide') AS suicide,
  EXTRACT_ARG(arg_set_id, 'debug.tick') AS tick
FROM slice
WHERE name = 'death'
ORDER BY ts;
