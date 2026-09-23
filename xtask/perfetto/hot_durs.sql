-- Typed duration spans in the active gameplay window. One row per instance.
-- Replay has no authority player_tick; its gameplay starts at world_ready.
WITH active AS (
  SELECT COALESCE(
    MIN(CASE WHEN name = 'player_tick' THEN ts END),
    MIN(CASE WHEN name = 'world_ready' THEN ts END),
    MIN(ts)
  ) AS start
  FROM slice
)
SELECT name, dur
FROM slice, active
WHERE name IN (
  'wall',
  'FixedUpdate',
  'Update',
  'PreUpdate',
  'PostUpdate',
  'Present',
  'Predict',
  'Effects',
  'Diag',
  'extract_wait', 'receive_render_world', 'extract_body', 'dispatch_render_world',
      'static_sun_fx', 'static_sun',
  'render_thread',
  'render_render',
  'post_execute',
  'post_rebuild',
  'colour_submit',
  'skin_model',
  'fx_update',
  'fx_present',
  'cull',
  'Load',
  'Receive',
  'Reconcile',
  'Input',
  'Send',
  'Ui',
  'Advance',
  'Ingress',
  'Gather',
  'Step',
  'Snapshot',
  'Fanout',
  'Bookkeeping'
)
AND ts >= active.start
ORDER BY name, dur;
