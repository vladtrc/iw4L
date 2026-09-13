-- G_FireMissile spawn.
SELECT
  EXTRACT_ARG(arg_set_id, 'debug.weapon') AS weapon,
  EXTRACT_ARG(arg_set_id, 'debug.weapon_kind') AS weapon_kind
FROM slice
WHERE name = 'projectile'
ORDER BY ts;
