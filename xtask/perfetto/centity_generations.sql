WITH replacements AS (
  SELECT EXTRACT_ARG(arg_set_id, 'debug.number') AS number,
         EXTRACT_ARG(arg_set_id, 'debug.old_generation') AS old_generation,
         EXTRACT_ARG(arg_set_id, 'debug.generation') AS generation
  FROM slice WHERE name = 'centity_replace'
), births AS (
  SELECT EXTRACT_ARG(arg_set_id, 'debug.number') AS number,
         EXTRACT_ARG(arg_set_id, 'debug.generation') AS generation,
         EXTRACT_ARG(arg_set_id, 'debug.survivors') AS survivors
  FROM slice WHERE name = 'centity_birth'
)
SELECT 'generation_changes' AS metric, COUNT(*) AS total FROM replacements
WHERE old_generation != generation
UNION ALL
SELECT 'corpse_generation_changes', COUNT(*) FROM replacements
WHERE old_generation != generation AND number >= 18 AND number < 26
UNION ALL
SELECT 'dynamic_reused_generation_births', COUNT(*) FROM
  (SELECT DISTINCT number, generation FROM births WHERE number >= 26 AND generation > 0)
UNION ALL
SELECT 'presentations_survived_replacement', COALESCE(SUM(survivors), 0) FROM births
UNION ALL
SELECT 'entity_births', COUNT(*) FROM births;
