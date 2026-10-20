-- The fitting simulator: doctrine presets kept by the coalition's admins, and each character's
-- own fits and fleets. `doc` is the versioned JSON the engine reads (spai_fit::doc).
CREATE TABLE IF NOT EXISTS sim_doctrines (
    id           TEXT PRIMARY KEY,
    side         TEXT NOT NULL CHECK (side IN ('imperium', 'enemy', 'public')),
    name         TEXT NOT NULL,
    ship_type_id BIGINT NOT NULL,
    tags         TEXT[] NOT NULL DEFAULT '{}',
    refit_keys   TEXT[] NOT NULL DEFAULT '{}',
    rev          INTEGER NOT NULL DEFAULT 1,
    doc          JSONB NOT NULL,
    updated_by   BIGINT NOT NULL,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS sim_fits (
    char_id      BIGINT NOT NULL,
    id           TEXT NOT NULL,
    name         TEXT NOT NULL,
    ship_type_id BIGINT NOT NULL,
    rev          INTEGER NOT NULL DEFAULT 1,
    doc          JSONB NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (char_id, id)
);
CREATE INDEX IF NOT EXISTS sim_fits_char_updated ON sim_fits (char_id, updated_at DESC);

CREATE TABLE IF NOT EXISTS sim_fleets (
    char_id      BIGINT NOT NULL,
    id           TEXT NOT NULL,
    name         TEXT NOT NULL,
    -- The main hull, for the list; 0 while a fleet has none.
    ship_type_id BIGINT NOT NULL DEFAULT 0,
    rev          INTEGER NOT NULL DEFAULT 1,
    doc          JSONB NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (char_id, id)
);
CREATE INDEX IF NOT EXISTS sim_fleets_char_updated ON sim_fleets (char_id, updated_at DESC);
