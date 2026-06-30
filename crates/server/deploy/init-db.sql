-- One-time database bootstrap for the EVE Spai battle-report server.
--
-- Run as the Postgres SUPERUSER inside your existing Postgres container
-- (replace <postgres-container> with its name):
--     docker exec -i <postgres-container> psql -U postgres < init-db.sql
--
-- SAFETY: this only CREATEs a brand-new role `eve_spai` and a brand-new database
-- `eve_spai`. It leaves any other databases on the instance untouched — there are no
-- DROP/ALTER/GRANT against them here. The schema (tables, indexes) is applied
-- automatically by the server on startup via embedded sqlx migrations, so nothing
-- schema-related belongs in here.
--
-- BEFORE RUNNING — confirm the names are free (should print no rows):
--     docker exec -i <postgres-container> psql -U postgres -c "\du eve_spai"
--     docker exec -i <postgres-container> psql -U postgres -c "\l eve_spai"
-- AND confirm any other databases on the instance are untouched afterwards:
--     docker exec -i <postgres-container> psql -U postgres -c "\l"

-- 1) Login role. REPLACE the password with the same value you put in DATABASE_URL.
--    `CREATE ROLE` is not idempotent; if it already exists, skip this statement (or
--    use the DO-block form below instead).
CREATE ROLE eve_spai LOGIN PASSWORD 'CHANGEME-matches-DATABASE_URL';

-- Idempotent alternative for re-runs (create only if missing):
-- DO $$
-- BEGIN
--   IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'eve_spai') THEN
--     CREATE ROLE eve_spai LOGIN PASSWORD 'CHANGEME-matches-DATABASE_URL';
--   END IF;
-- END
-- $$;

-- 2) Database owned by that role. `CREATE DATABASE` cannot run inside a DO/IF block
--    and is not idempotent — if it already exists you'll get an error you can ignore.
--    To gate it, check first:
--      SELECT 1 FROM pg_database WHERE datname = 'eve_spai';
CREATE DATABASE eve_spai OWNER eve_spai;

-- Nothing else. Do NOT grant cross-database privileges; eve_spai owns its own DB and
-- any other databases on the instance are deliberately left alone.
