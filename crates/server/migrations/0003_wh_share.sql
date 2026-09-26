-- Wormhole sharing groups. Everything a group says is end-to-end encrypted by its members; this
-- server stores ciphertext and decides who may fetch it. Group names live inside the encrypted log.

CREATE TABLE IF NOT EXISTS wh_groups (
    id         TEXT PRIMARY KEY,
    owner      BIGINT NOT NULL,
    epoch      INT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS wh_members (
    group_id  TEXT NOT NULL REFERENCES wh_groups(id) ON DELETE CASCADE,
    char_id   BIGINT NOT NULL,
    name      TEXT NOT NULL,
    role      TEXT NOT NULL,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, char_id)
);

-- The group key of each epoch, wrapped to one member's public key.
CREATE TABLE IF NOT EXISTS wh_keys (
    group_id TEXT NOT NULL REFERENCES wh_groups(id) ON DELETE CASCADE,
    epoch    INT NOT NULL,
    char_id  BIGINT NOT NULL,
    wrapped  TEXT NOT NULL,
    PRIMARY KEY (group_id, epoch, char_id)
);

CREATE TABLE IF NOT EXISTS wh_invites (
    id         TEXT PRIMARY KEY,
    group_id   TEXT NOT NULL REFERENCES wh_groups(id) ON DELETE CASCADE,
    created_by BIGINT NOT NULL,
    blob       TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    used_by    BIGINT
);

CREATE TABLE IF NOT EXISTS wh_join_requests (
    group_id   TEXT NOT NULL REFERENCES wh_groups(id) ON DELETE CASCADE,
    char_id    BIGINT NOT NULL,
    name       TEXT NOT NULL,
    invite_id  TEXT NOT NULL,
    body       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, char_id)
);

-- The log. `keep` marks membership entries, which outlive the data retention.
CREATE TABLE IF NOT EXISTS wh_ops (
    seq        BIGSERIAL PRIMARY KEY,
    group_id   TEXT NOT NULL REFERENCES wh_groups(id) ON DELETE CASCADE,
    op_id      TEXT NOT NULL,
    epoch      INT NOT NULL,
    author     BIGINT NOT NULL,
    keep       BOOL NOT NULL,
    blob       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (group_id, op_id)
);
CREATE INDEX IF NOT EXISTS wh_ops_group_seq ON wh_ops (group_id, seq);
