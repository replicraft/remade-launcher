-- An app's own place on the update chain (manual, on-launch, daemon). Empty
-- follows the global mode. A pin is not stored here: it stays in
-- `pinned_release_id`, which also says which release it holds.
ALTER TABLE app_update_policy ADD COLUMN update_mode TEXT;
