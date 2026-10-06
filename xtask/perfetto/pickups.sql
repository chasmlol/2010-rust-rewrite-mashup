-- Item pickups granted this tick.
SELECT
  name AS event,
  EXTRACT_ARG(arg_set_id, 'debug.picker_pm_type') AS picker_pm_type
FROM slice
WHERE name IN ('pickup', 'pickup_rejected')
ORDER BY ts;
