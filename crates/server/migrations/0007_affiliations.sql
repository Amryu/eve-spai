-- The last alliance ESI gave for each character, so a restart while ESI is down still knows who
-- is in The Imperium.
CREATE TABLE IF NOT EXISTS affiliations (
    character_id BIGINT PRIMARY KEY,
    alliance_id  BIGINT,
    fetched_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);
