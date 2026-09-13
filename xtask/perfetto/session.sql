-- Session occupancy events at producers (not a dump census).
SELECT
  ts,
  name,
  EXTRACT_ARG(arg_set_id, 'debug.reason') AS reason,
  EXTRACT_ARG(arg_set_id, 'debug.zone') AS zone,
  EXTRACT_ARG(arg_set_id, 'debug.has_world') AS has_world,
  EXTRACT_ARG(arg_set_id, 'debug.spawned') AS spawned,
  EXTRACT_ARG(arg_set_id, 'debug.gpu_plan') AS gpu_plan,
  EXTRACT_ARG(arg_set_id, 'debug.glass_n') AS glass_n,
  EXTRACT_ARG(arg_set_id, 'debug.running') AS running,
  EXTRACT_ARG(arg_set_id, 'debug.movers') AS movers,
  EXTRACT_ARG(arg_set_id, 'debug.loopback_pending') AS loopback_pending,
  EXTRACT_ARG(arg_set_id, 'debug.booted') AS booted,
  EXTRACT_ARG(arg_set_id, 'debug.alias') AS alias,
  EXTRACT_ARG(arg_set_id, 'debug.id') AS id,
  EXTRACT_ARG(arg_set_id, 'debug.phase') AS phase,
  EXTRACT_ARG(arg_set_id, 'debug.target') AS target,
  EXTRACT_ARG(arg_set_id, 'debug.present') AS present,
  EXTRACT_ARG(arg_set_id, 'debug.quit_on_end') AS quit_on_end,
  EXTRACT_ARG(arg_set_id, 'debug.runs_authority') AS runs_authority,
  EXTRACT_ARG(arg_set_id, 'debug.present_choice') AS present_choice,
  EXTRACT_ARG(arg_set_id, 'debug.presented_has_ps') AS presented_has_ps
FROM slice
WHERE name IN (
  'match_torn',
  'match_installed',
  'world_hold',
  'world_ready',
  'sim_hold',
  'ambient_hold',
  'ambient_boot',
  'swap',
  'theater',
  'cgame_hold'
)
ORDER BY ts;
