-- Keep-set envelopes and hot leaves (P4 / I12). Missing names are absent
-- from this recording, not a SQL error — the runner flags them.
SELECT
  name,
  COUNT(*) AS n,
  CAST(AVG(dur) / 1e6 AS REAL) AS avg_ms,
  CAST(MAX(dur) / 1e6 AS REAL) AS max_ms,
  CAST(SUM(dur) / 1e6 AS REAL) AS sum_ms
FROM slice
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
  'extract_wait',
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
GROUP BY name
ORDER BY sum_ms DESC;
