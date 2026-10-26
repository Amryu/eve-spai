-- Doctrines as an alliance flies them, each a document holding its fits (spai_fit::doctrine),
-- kept by the admins and read by the side they belong to; and each character's own doctrines.
CREATE TABLE IF NOT EXISTS sim_supplied (
    id           TEXT PRIMARY KEY,
    side         TEXT NOT NULL CHECK (side IN ('imperium', 'enemy', 'public')),
    alliance     TEXT NOT NULL,
    category     TEXT NOT NULL,
    name         TEXT NOT NULL,
    rev          INTEGER NOT NULL DEFAULT 1,
    doc          JSONB NOT NULL,
    updated_by   BIGINT NOT NULL,
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS sim_own_doctrines (
    char_id      BIGINT NOT NULL,
    id           TEXT NOT NULL,
    name         TEXT NOT NULL,
    ship_type_id BIGINT NOT NULL DEFAULT 0,
    rev          INTEGER NOT NULL DEFAULT 1,
    doc          JSONB NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (char_id, id)
);
CREATE INDEX IF NOT EXISTS sim_own_doctrines_char_updated ON sim_own_doctrines (char_id, updated_at DESC);
