-- Battle-report storage. Run automatically on startup via sqlx::migrate!.

CREATE TABLE IF NOT EXISTS battle_reports (
    id               TEXT PRIMARY KEY,
    uploader_char_id BIGINT NOT NULL,
    uploader_name    TEXT NOT NULL,
    title            TEXT,
    unlisted         BOOL NOT NULL DEFAULT false,
    format_version   INT NOT NULL,
    content_sha256   TEXT NOT NULL,
    doc              JSONB NOT NULL,
    started_at       TIMESTAMPTZ,
    ended_at         TIMESTAMPTZ,
    systems          TEXT[] NOT NULL DEFAULT '{}',
    system_ids       BIGINT[] NOT NULL DEFAULT '{}',
    total_isk        DOUBLE PRECISION NOT NULL DEFAULT 0,
    kills            INT NOT NULL DEFAULT 0,
    participants     INT NOT NULL DEFAULT 0,
    side_names       TEXT[] NOT NULL DEFAULT '{}',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_viewed_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    views            BIGINT NOT NULL DEFAULT 0
);

-- A character cannot store the same canonical document twice (idempotent re-upload).
CREATE UNIQUE INDEX IF NOT EXISTS battle_reports_uploader_sha
    ON battle_reports (uploader_char_id, content_sha256);

CREATE INDEX IF NOT EXISTS battle_reports_systems_gin    ON battle_reports USING gin (systems);
CREATE INDEX IF NOT EXISTS battle_reports_side_names_gin ON battle_reports USING gin (side_names);
CREATE INDEX IF NOT EXISTS battle_reports_created_at     ON battle_reports (created_at DESC);
CREATE INDEX IF NOT EXISTS battle_reports_total_isk      ON battle_reports (total_isk DESC);
CREATE INDEX IF NOT EXISTS battle_reports_uploader       ON battle_reports (uploader_char_id);

-- Per-character upload counters, one row per rolling-hour window.
CREATE TABLE IF NOT EXISTS upload_quota (
    char_id      BIGINT NOT NULL,
    window_start TIMESTAMPTZ NOT NULL,
    count        INT NOT NULL,
    PRIMARY KEY (char_id, window_start)
);
