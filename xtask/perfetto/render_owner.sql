-- One explicitly selected render owner. The runner requires manifest.focus.
WITH plans AS (
  SELECT
    ts,
    EXTRACT_ARG(arg_set_id, 'debug.frame_id') AS frame_id,
    EXTRACT_ARG(arg_set_id, 'debug.world_generation') AS world_generation,
    EXTRACT_ARG(arg_set_id, 'debug.owner_kind') AS owner_kind,
    EXTRACT_ARG(arg_set_id, 'debug.owner_id') AS owner_id,
    EXTRACT_ARG(arg_set_id, 'debug.model') AS model,
    EXTRACT_ARG(arg_set_id, 'debug.outcome') AS plan_outcome,
    EXTRACT_ARG(arg_set_id, 'debug.object_id') AS object_id,
    EXTRACT_ARG(arg_set_id, 'debug.camera_x') AS camera_x,
    EXTRACT_ARG(arg_set_id, 'debug.camera_y') AS camera_y,
    EXTRACT_ARG(arg_set_id, 'debug.camera_z') AS camera_z,
    EXTRACT_ARG(arg_set_id, 'debug.camera_hidden') AS camera_hidden,
    EXTRACT_ARG(arg_set_id, 'debug.lighting_handle') AS lighting_handle,
    EXTRACT_ARG(arg_set_id, 'debug.planned_surfaces') AS planned_surfaces
  FROM slice
  WHERE name = 'render_owner_plan'
),
submits AS (
  SELECT
    ts,
    EXTRACT_ARG(arg_set_id, 'debug.frame_id') AS frame_id,
    EXTRACT_ARG(arg_set_id, 'debug.owner_id') AS owner_id,
    EXTRACT_ARG(arg_set_id, 'debug.outcome') AS submit_outcome,
    EXTRACT_ARG(arg_set_id, 'debug.materials') AS materials,
    EXTRACT_ARG(arg_set_id, 'debug.material_textures') AS material_textures,
    EXTRACT_ARG(arg_set_id, 'debug.product_surfaces') AS product_surfaces,
    EXTRACT_ARG(arg_set_id, 'debug.execution_ready_surfaces') AS execution_ready_surfaces,
    EXTRACT_ARG(arg_set_id, 'debug.prepared_surfaces') AS prepared_surfaces,
    EXTRACT_ARG(arg_set_id, 'debug.prepared_passes') AS prepared_passes,
    EXTRACT_ARG(arg_set_id, 'debug.drawn_passes') AS drawn_passes
  FROM slice
  WHERE name = 'render_owner_submit'
),
keys AS (
  SELECT frame_id, owner_id FROM plans
  UNION
  SELECT frame_id, owner_id FROM submits
),
plan_counts AS (
  SELECT frame_id, owner_id, COUNT(*) AS plan_events
  FROM plans
  GROUP BY frame_id, owner_id
),
submit_counts AS (
  SELECT frame_id, owner_id, COUNT(*) AS submit_events
  FROM submits
  GROUP BY frame_id, owner_id
),
submit_bounds AS (
  SELECT MAX(frame_id) AS last_submit_frame
  FROM submits
)
SELECT
  k.frame_id,
  p.world_generation,
  p.owner_kind,
  k.owner_id,
  p.model,
  p.object_id,
  p.plan_outcome,
  s.submit_outcome,
  s.materials,
  s.material_textures,
  p.camera_x,
  p.camera_y,
  p.camera_z,
  p.camera_hidden,
  p.lighting_handle,
  p.planned_surfaces,
  s.product_surfaces,
  s.execution_ready_surfaces,
  s.prepared_surfaces,
  s.prepared_passes,
  s.drawn_passes,
  COALESCE(pc.plan_events, 0) AS plan_events,
  COALESCE(sc.submit_events, 0) AS submit_events,
  CASE
    WHEN COALESCE(pc.plan_events, 0) = 0
      THEN 'missing_plan'
    WHEN pc.plan_events > 1
      THEN 'duplicate_plan'
    WHEN sc.submit_events = 1
      THEN 'closed'
    WHEN sc.submit_events > 1
      THEN 'duplicate_submit'
    WHEN k.frame_id > COALESCE(b.last_submit_frame, -1)
      THEN 'trace_tail_not_presented'
    ELSE 'missing_submit'
  END AS closure
FROM keys k
LEFT JOIN plans p USING (frame_id, owner_id)
LEFT JOIN submits s USING (frame_id, owner_id)
LEFT JOIN plan_counts pc USING (frame_id, owner_id)
LEFT JOIN submit_counts sc USING (frame_id, owner_id)
CROSS JOIN submit_bounds b
ORDER BY COALESCE(p.ts, s.ts);
