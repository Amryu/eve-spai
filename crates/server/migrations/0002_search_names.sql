-- Comprehensive participant search. `side_names` only holds the two side LABELS (the
-- dominant alliance/coalition per side), so non-dominant alliances/corps and individual
-- pilots can never be matched. `search_names` is the full, lower-cased set of every party
-- name (alliances/corps on the sides and on every killmail) plus every pilot name in the
-- battle. Populated on insert and backfilled for pre-existing rows on startup.

ALTER TABLE battle_reports
    ADD COLUMN IF NOT EXISTS search_names TEXT[] NOT NULL DEFAULT '{}';

CREATE INDEX IF NOT EXISTS battle_reports_search_names_gin
    ON battle_reports USING gin (search_names);
