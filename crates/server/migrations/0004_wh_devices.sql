-- Protocol 2: a member is a character with one or more devices, each with its own keys, and a
-- viewer role that reads without writing. What was one member's single key becomes their first
-- device, named 'legacy' until that device claims it with its real id.

CREATE TABLE IF NOT EXISTS wh_devices (
    group_id  TEXT NOT NULL,
    char_id   BIGINT NOT NULL,
    device_id TEXT NOT NULL,
    label     TEXT NOT NULL DEFAULT '',
    added_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (group_id, char_id, device_id),
    FOREIGN KEY (group_id, char_id) REFERENCES wh_members (group_id, char_id) ON DELETE CASCADE
);
INSERT INTO wh_devices (group_id, char_id, device_id) SELECT group_id, char_id, 'legacy' FROM wh_members ON CONFLICT DO NOTHING;

ALTER TABLE wh_keys ADD COLUMN device_id TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE wh_keys DROP CONSTRAINT wh_keys_pkey;
ALTER TABLE wh_keys ADD PRIMARY KEY (group_id, epoch, char_id, device_id);

ALTER TABLE wh_join_requests ADD COLUMN device_id TEXT NOT NULL DEFAULT 'legacy';
ALTER TABLE wh_join_requests ADD COLUMN label TEXT NOT NULL DEFAULT '';
ALTER TABLE wh_join_requests DROP CONSTRAINT wh_join_requests_pkey;
ALTER TABLE wh_join_requests ADD PRIMARY KEY (group_id, char_id, device_id);

ALTER TABLE wh_ops ADD COLUMN device_id TEXT;

ALTER TABLE wh_members ADD CONSTRAINT wh_members_role CHECK (role IN ('owner', 'admin', 'member', 'viewer'));
