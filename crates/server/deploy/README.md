# EVE Spai battle-report server — production runbook

Ordered, copy-pasteable steps to deploy `eve-spai-br` onto the host. Run each step,
then run its **Check** before moving on. Nothing here is run automatically — a human
(or a later supervised apply) executes it on the box.

## Target topology (already in place — do not change)

- **Cloudflare Tunnel** (`cloudflared`) terminates TLS and forwards to the `nginx`
  container on the `frontend` network. No cloudflared change is needed.
- **nginx** container `nginx` on `frontend`; config bind-mounted from host
  `/srv/nginx/conf` → `/etc/nginx`. The `eve-spai.com` vhost lives at
  `/etc/nginx/sites-available/eve-spai.com` and serves a static site at `/`.
- **Postgres** container `postgres-db-1` (postgis/postgis:16-3.4) on `backend`. The
  databases `postgres` / `nexus` / `nexus_users` belong to the unrelated
  entropia-nexus project and **must not be touched**.
- **This service** `eve-spai-br` joins BOTH `frontend` (nginx reaches it by DNS) and
  `backend` (it reaches Postgres by DNS), listens internally on `:8090`, and is not
  host-published.

All commands assume you are in the server crate directory on the box:

```sh
cd ~/eve-spai/crates/server
```

---

## 1. Get the code onto the box

```sh
# First time:
git clone https://github.com/Amryu/eve-spai ~/eve-spai
# Or update an existing checkout:
git -C ~/eve-spai fetch --all && git -C ~/eve-spai pull --ff-only
cd ~/eve-spai/crates/server
```

**Check** — the deploy files are present:

```sh
ls deploy/   # docker-compose.yml  .env.example  init-db.sql  nginx-eve-spai-br.conf  README.md
test -f Dockerfile && echo "Dockerfile OK"
```

---

## 2. Create the `eve_spai` database + role

Pick a strong password and put the SAME value here and in `.env` (step 3). Edit the
placeholder in `deploy/init-db.sql` first (`CHANGEME-matches-DATABASE_URL`).

Confirm the names are free and the neighbours exist **before**:

```sh
docker exec -i postgres-db-1 psql -U postgres -c "\du eve_spai"   # expect: no such role
docker exec -i postgres-db-1 psql -U postgres -c "\l eve_spai"    # expect: no rows
docker exec -i postgres-db-1 psql -U postgres -c "\l" | grep -E 'nexus|nexus_users|postgres'
```

Apply:

```sh
docker exec -i postgres-db-1 psql -U postgres < deploy/init-db.sql
```

**Check** — our DB exists and the neighbours are untouched:

```sh
docker exec -i postgres-db-1 psql -U postgres -c "\l" \
  | grep -E 'eve_spai|nexus|nexus_users|postgres'
# Expect: eve_spai (owner eve_spai) present; nexus / nexus_users / postgres still listed, unchanged.
```

---

## 3. Create `.env` with real secrets

```sh
cp deploy/.env.example deploy/.env
# Edit deploy/.env:
#   DATABASE_URL  -> set the password to the one from step 2
#   BR_SESSION_SECRET -> generate one:
openssl rand -hex 32        # paste into BR_SESSION_SECRET
chmod 600 deploy/.env       # root-only; never commit
```

**Check**:

```sh
test "$(stat -c '%a' deploy/.env)" = "600" && echo "perms OK"
grep -q 'CHANGEME' deploy/.env && echo "STILL HAS PLACEHOLDERS — fix before continuing" || echo "no placeholders OK"
git -C ~/eve-spai status --porcelain deploy/.env   # expect: ignored / nothing to commit
```

---

## 4. Build and start the container

```sh
docker compose -f deploy/docker-compose.yml up -d --build
```

The server runs the embedded sqlx migrations against `eve_spai` on startup — no
separate migrate step.

**Check** — container is healthy, joined both networks, and `/healthz` answers:

```sh
docker compose -f deploy/docker-compose.yml ps          # State: running / healthy
docker logs eve-spai-br --tail 30                        # expect "battle-report API listening" + migrations applied

# Joined BOTH networks?
docker inspect -f '{{range $n,$_ := .NetworkSettings.Networks}}{{$n}} {{end}}' eve-spai-br
# expect: frontend backend

# /healthz reachable over the frontend network (the way nginx will reach it):
docker run --rm --network frontend curlimages/curl -fsS http://eve-spai-br:8090/healthz
# expect: ok
```

If it can't reach Postgres, re-check the password in `DATABASE_URL` vs step 2 and that
`postgres-db-1` is on `backend`.

---

## 5. Wire up nginx

Open the vhost and paste the snippet's `location /api/br` and `location /br` blocks
(plus the realip lines) **inside** the existing `server { ... }` for eve-spai.com,
next to the static `location /`:

```sh
# Host path that is bind-mounted into the nginx container:
sudo $EDITOR /srv/nginx/conf/sites-available/eve-spai.com
# paste from deploy/nginx-eve-spai-br.conf
```

Validate, then reload gracefully (no dropped connections):

```sh
docker exec nginx nginx -t          # must print "syntax is ok" / "test is successful"
docker exec nginx nginx -s reload   # graceful reload
```

**Check** — config tested OK before reload. If `nginx -t` fails, DO NOT reload; fix
the snippet first.

---

## 6. Smoke test end-to-end

```sh
curl -fsSI https://eve-spai.com/br            # 200, served by eve-spai-br
curl -fsS  https://eve-spai.com/api/br        # JSON listing (200)
curl -fsSI https://eve-spai.com/              # static site STILL 200, unchanged
```

Also confirm the co-tenant sites are unaffected:

```sh
# entropia-nexus and any other vhosts should still respond exactly as before.
docker logs nginx --tail 20    # no new errors after reload
```

---

## 7. Daily retention (optional housekeeping)

Prune reports no one has viewed in a long time. The interval below (`30 years`) is a
deliberately huge no-op placeholder — shorten it to your real policy before enabling.

One-liner you can drop into the host crontab (runs 04:17 daily):

```cron
17 4 * * *  docker exec -i postgres-db-1 psql -U eve_spai -d eve_spai -c "DELETE FROM battle_reports WHERE last_viewed_at < now() - interval '30 years';" >> /var/log/eve-spai-retention.log 2>&1
```

Install it:

```sh
( crontab -l 2>/dev/null; echo "17 4 * * *  docker exec -i postgres-db-1 psql -U eve_spai -d eve_spai -c \"DELETE FROM battle_reports WHERE last_viewed_at < now() - interval '30 years';\" >> /var/log/eve-spai-retention.log 2>&1" ) | crontab -
```

**Check** — dry-run the count first so you know what a real interval would delete:

```sh
docker exec -i postgres-db-1 psql -U eve_spai -d eve_spai \
  -c "SELECT count(*) FROM battle_reports WHERE last_viewed_at < now() - interval '30 years';"
```

---

## Rollback — return the box to exactly its prior state

1. **Stop + remove the service** (also removes its image build):

   ```sh
   cd ~/eve-spai/crates/server
   docker compose -f deploy/docker-compose.yml down
   docker image rm eve-spai-br:latest 2>/dev/null || true
   ```

2. **Revert the nginx snippet** — delete the `location /br`, `location /api/br`, and
   the realip lines you pasted in step 5, then re-test and graceful-reload:

   ```sh
   sudo $EDITOR /srv/nginx/conf/sites-available/eve-spai.com   # remove the pasted blocks
   docker exec nginx nginx -t && docker exec nginx nginx -s reload
   ```

   `https://eve-spai.com/` (static) and the co-tenant sites are now exactly as before;
   `/br` and `/api/br` return to whatever the static `/` did previously (typically 404).

3. **Drop our database + role** (leaves `postgres` / `nexus` / `nexus_users` untouched):

   ```sh
   docker exec -i postgres-db-1 psql -U postgres -c "DROP DATABASE IF EXISTS eve_spai;"
   docker exec -i postgres-db-1 psql -U postgres -c "DROP ROLE IF EXISTS eve_spai;"
   ```

4. **Remove secrets and crontab entry**:

   ```sh
   rm -f ~/eve-spai/crates/server/deploy/.env
   crontab -l 2>/dev/null | grep -v 'eve-spai-retention' | crontab -
   ```

**Check** — nothing of ours remains; neighbours intact:

```sh
docker ps -a | grep eve-spai-br || echo "service gone OK"
docker exec -i postgres-db-1 psql -U postgres -c "\l" | grep -E 'nexus|nexus_users|postgres'  # still present
curl -fsSI https://eve-spai.com/   # static site still 200
```
