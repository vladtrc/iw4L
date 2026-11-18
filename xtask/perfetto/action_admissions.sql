-- One match per trace; sequence numbers restart at the match boundary.
WITH admitted AS (
  SELECT EXTRACT_ARG(arg_set_id, 'debug.client') AS client,
         EXTRACT_ARG(arg_set_id, 'debug.sequence') AS sequence
  FROM slice WHERE name = 'client_action_admitted'
), duplicates AS (
  SELECT client, sequence, COUNT(*) AS copies
  FROM admitted GROUP BY client, sequence HAVING COUNT(*) > 1
)
SELECT 'admitted' AS metric, COUNT(*) AS total FROM admitted
UNION ALL SELECT 'duplicate_admissions', COUNT(*) FROM duplicates;
