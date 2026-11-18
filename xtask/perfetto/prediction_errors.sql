WITH checks AS (
 SELECT EXTRACT_ARG(arg_set_id, 'debug.arm') AS arm, COUNT(*) AS checked
 FROM slice WHERE name = 'prediction_check' GROUP BY arm
), errors AS (
 SELECT EXTRACT_ARG(arg_set_id, 'debug.arm') AS arm,
        EXTRACT_ARG(arg_set_id, 'debug.field') AS field,
        EXTRACT_ARG(arg_set_id, 'debug.distance') AS distance
 FROM slice WHERE name = 'prediction_field_error'
), by_field AS (
 SELECT arm, field, COUNT(*) AS differences, AVG(distance) AS mean_distance,
        MAX(distance) AS max_distance,
        SUM(CASE WHEN distance IS NULL OR ABS(distance) > 1.7976931348623157e308 THEN 1 ELSE 0 END) AS nonfinite_errors,
        PERCENTILE(CASE WHEN ABS(distance) <= 1.7976931348623157e308 THEN distance END, 95) AS p95_finite_distance
 FROM errors GROUP BY arm, field
)
SELECT checks.arm, COALESCE(field, '<no differences>') AS field, checked,
       COALESCE(differences,0) AS differences, COALESCE(mean_distance,0) AS mean_distance,
       COALESCE(max_distance,0) AS max_distance, COALESCE(nonfinite_errors,0) AS nonfinite_errors,
       p95_finite_distance
FROM checks LEFT JOIN by_field USING(arm) ORDER BY checks.arm, field;
