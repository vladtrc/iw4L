-- Any nonzero loss/error makes a recording incomplete rather than approximate.
SELECT name, value, severity, source
FROM stats
WHERE value != 0
  AND (
    severity = 'error'
    OR name GLOB '*overwrit*'
    OR name GLOB '*drop*'
  )
ORDER BY name, source;
