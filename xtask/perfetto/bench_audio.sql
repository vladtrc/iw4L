WITH audio AS (
  SELECT EXTRACT_ARG(arg_set_id, 'debug.data') AS data
  FROM slice WHERE name = 'load_audio' ORDER BY ts DESC LIMIT 1
)
SELECT fullkey AS field, atom AS value FROM audio, JSON_TREE(data)
WHERE type NOT IN ('object', 'array') ORDER BY fullkey;
