-- Who besides its owner may read a saved fit. A share goes with the fit.
CREATE TABLE IF NOT EXISTS sim_fit_shares (
    owner_id     BIGINT NOT NULL,
    fit_id       TEXT NOT NULL,
    owner_name   TEXT NOT NULL,
    grantee_id   BIGINT NOT NULL,
    grantee_name TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (owner_id, fit_id, grantee_id),
    FOREIGN KEY (owner_id, fit_id) REFERENCES sim_fits (char_id, id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS sim_fit_shares_grantee ON sim_fit_shares (grantee_id);

-- A fit's link, made the first time it is shared: it opens the owner's current fit for the
-- owner, those it is shared with and, when the owner allows it, anyone holding it.
ALTER TABLE sim_fits ADD COLUMN IF NOT EXISTS share_id TEXT UNIQUE;
ALTER TABLE sim_fits ADD COLUMN IF NOT EXISTS link_public BOOLEAN NOT NULL DEFAULT false;
ALTER TABLE sim_fits ADD COLUMN IF NOT EXISTS owner_name TEXT;
