-- G-LIVE-1 player snapshots. Zero-width slices at PublishSnapshot.
-- Args are Perfetto debug annotations (`debug.<name>`).
SELECT
  EXTRACT_ARG(arg_set_id, 'debug.client_id') AS client_id,
  EXTRACT_ARG(arg_set_id, 'debug.is_bot') AS is_bot,
  EXTRACT_ARG(arg_set_id, 'debug.time_ms') AS time_ms,
  EXTRACT_ARG(arg_set_id, 'debug.level_time_ms') AS level_time_ms,
  EXTRACT_ARG(arg_set_id, 'debug.origin_x') AS origin_x,
  EXTRACT_ARG(arg_set_id, 'debug.origin_y') AS origin_y,
  EXTRACT_ARG(arg_set_id, 'debug.origin_z') AS origin_z,
  EXTRACT_ARG(arg_set_id, 'debug.vz') AS vz,
  EXTRACT_ARG(arg_set_id, 'debug.yaw') AS yaw,
  EXTRACT_ARG(arg_set_id, 'debug.pitch') AS pitch,
  EXTRACT_ARG(arg_set_id, 'debug.lifecycle') AS lifecycle,
  EXTRACT_ARG(arg_set_id, 'debug.jump_time') AS jump_time,
  EXTRACT_ARG(arg_set_id, 'debug.buttons') AS buttons,
  EXTRACT_ARG(arg_set_id, 'debug.weaponstate') AS weaponstate,
  EXTRACT_ARG(arg_set_id, 'debug.ammo_clip') AS ammo_clip,
  EXTRACT_ARG(arg_set_id, 'debug.walking') AS walking,
  EXTRACT_ARG(arg_set_id, 'debug.legs_anim') AS legs_anim,
  EXTRACT_ARG(arg_set_id, 'debug.torso_anim') AS torso_anim
FROM slice
WHERE name = 'player_tick'
ORDER BY ts;
