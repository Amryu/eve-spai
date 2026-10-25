# EVE Spai battle-report server — deployment guide

A generic, copy-pasteable runbook to deploy `eve-spai-br` behind an existing nginx +
Postgres stack. The exact container names, host paths, and TLS terminator are whatever
your own box uses — substitute your values for the `<placeholders>` below. Run each
step, then run its **Check** before moving on.

## Target topology

This guide assumes you already run, on the same host:

- A **TLS terminator / reverse proxy** in front of nginx (e.g. a tunnel or load
  balancer) that forwards public HTTPS to your **nginx** container. No change is needed
  there.
- An **nginx** container (call it `<nginx-container>`) on a docker network shared with
  this service (referred to as `frontend` below). Its config is bind-mounted from a host
  directory (`<nginx-config-dir>`, e.g. `<nginx-config-dir>/sites-available/eve-spai.com`)
  and it already serves a static site at `/` for the `eve-spai.com` vhost.
- A **Postgres** container (call it `<postgres-container>`, PostGIS works fine) on a
  docker network shared with this service (referred to as `backend` below). It may host
  other unrelated databases — this deploy never touches them.
- **This service** `eve-spai-br` joins BOTH networks: `frontend` (so nginx reaches it by
  DNS) and `backend` (so it reaches Postgres by DNS). It listens internally on `:8090`
  and is not host-published.

`frontend` / `backend` are just placeholder names for whatever docker networks your
nginx and Postgres already live on; adjust `docker-compose.yml` to match.

All commands assume you are in the server crate directory:

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

Confirm the names are free **before** (replace `<postgres-container>` with your Postgres
container name):

```sh
docker exec -i <postgres-container> psql -U postgres -c "\du eve_spai"   # expect: no such role
docker exec -i <postgres-container> psql -U postgres -c "\l eve_spai"    # expect: no rows
```

Apply:

```sh
docker exec -i <postgres-container> psql -U postgres < deploy/init-db.sql
```

**Check** — our DB exists; any other databases on the instance are untouched:

```sh
docker exec -i <postgres-container> psql -U postgres -c "\l" | grep -E 'eve_spai'
# Expect: eve_spai (owner eve_spai) present. Other databases on the instance are listed unchanged.
```

> **Collation tip:** if Postgres refuses to create the new database with a
> "collation version mismatch" error, refresh the template once and retry:
> `docker exec -i <postgres-container> psql -U postgres -c "ALTER DATABASE template1 REFRESH COLLATION VERSION;"`

---

## 3. Create `.env` with real secrets

```sh
cp deploy/.env.example deploy/.env
# Edit deploy/.env:
#   DATABASE_URL  -> set the host to your Postgres container/host and the password from step 2
#   BR_SESSION_SECRET -> generate one:
openssl rand -hex 32        # paste into BR_SESSION_SECRET
chmod 600 deploy/.env       # owner-only; never commit
```

**Check**:

```sh
test "$(stat -c '%a' deploy/.env)" = "600" && echo "perms OK"
grep -q 'CHANGEME' deploy/.env && echo "STILL HAS PLACEHOLDERS — fix before continuing" || echo "no placeholders OK"
git -C ~/eve-spai status --porcelain deploy/.env   # expect: ignored / nothing to commit
```

---

## 4. Build and start the container

Edit `deploy/docker-compose.yml` so the external `frontend` / `backend` networks match
the names of the docker networks your nginx and Postgres actually live on, then:

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
# expect: frontend backend (or whatever you named them)

# /healthz reachable over the frontend network (the way nginx will reach it):
docker run --rm --network frontend curlimages/curl -fsS http://eve-spai-br:8090/healthz
# expect: ok
```

If it can't reach Postgres, re-check the host + password in `DATABASE_URL` vs step 2 and
that your Postgres container is on the `backend` network.

---

## 5. Wire up nginx

Open the `eve-spai.com` vhost and paste the snippet's `location /api/br` and
`location /br` blocks (plus the realip lines) **inside** the existing `server { ... }`,
next to the static `location /`:

```sh
# Host path that is bind-mounted into your nginx container:
sudo $EDITOR <nginx-config-dir>/sites-available/eve-spai.com
# paste from deploy/nginx-eve-spai-br.conf
```

Validate, then reload gracefully (no dropped connections):

```sh
docker exec <nginx-container> nginx -t          # must print "syntax is ok" / "test is successful"
docker exec <nginx-container> nginx -s reload   # graceful reload
```

**Check** — config tested OK before reload. If `nginx -t` fails, DO NOT reload; fix
the snippet first.

---

## 6. Smoke test end-to-end

```sh
curl -fsSI https://eve-spai.com/br            # 200, served by eve-spai-br
curl -fsS  https://eve-spai.com/api/br        # JSON listing (200)
curl -fsSI https://eve-spai.com/              # static site STILL 200, unchanged
docker logs <nginx-container> --tail 20       # no new errors after reload
```

Any other vhosts you host should still respond exactly as before.

---

## 7. Daily retention (optional housekeeping)

Prune reports no one has viewed in a long time. The interval below (`30 years`) is a
deliberately huge no-op placeholder — shorten it to your real policy before enabling.

One-liner you can drop into the host crontab (runs 04:17 daily):

```cron
17 4 * * *  docker exec -i <postgres-container> psql -U eve_spai -d eve_spai -c "DELETE FROM battle_reports WHERE last_viewed_at < now() - interval '30 years';" >> /var/log/eve-spai-retention.log 2>&1
```

Install it:

```sh
( crontab -l 2>/dev/null; echo "17 4 * * *  docker exec -i <postgres-container> psql -U eve_spai -d eve_spai -c \"DELETE FROM battle_reports WHERE last_viewed_at < now() - interval '30 years';\" >> /var/log/eve-spai-retention.log 2>&1" ) | crontab -
```

**Check** — dry-run the count first so you know what a real interval would delete:

```sh
docker exec -i <postgres-container> psql -U eve_spai -d eve_spai \
  -c "SELECT count(*) FROM battle_reports WHERE last_viewed_at < now() - interval '30 years';"
```

---

## Rollback — return the box to its prior state

1. **Stop + remove the service** (also removes its image build):

   ```sh
   cd ~/eve-spai/crates/server
   docker compose -f deploy/docker-compose.yml down
   docker image rm eve-spai-br:latest 2>/dev/null || true
   ```

2. **Revert the nginx snippet** — delete the `location /br`, `location /api/br`, and
   the realip lines you pasted in step 5, then re-test and graceful-reload:

   ```sh
   sudo $EDITOR <nginx-config-dir>/sites-available/eve-spai.com   # remove the pasted blocks
   docker exec <nginx-container> nginx -t && docker exec <nginx-container> nginx -s reload
   ```

   `https://eve-spai.com/` (static) and any other vhosts are now exactly as before;
   `/br` and `/api/br` return to whatever the static `/` did previously (typically 404).

3. **Drop our database + role** (leaves any other databases on the instance untouched):

   ```sh
   docker exec -i <postgres-container> psql -U postgres -c "DROP DATABASE IF EXISTS eve_spai;"
   docker exec -i <postgres-container> psql -U postgres -c "DROP ROLE IF EXISTS eve_spai;"
   ```

4. **Remove secrets and crontab entry**:

   ```sh
   rm -f ~/eve-spai/crates/server/deploy/.env
   crontab -l 2>/dev/null | grep -v 'eve-spai-retention' | crontab -
   ```

**Check** — nothing of ours remains:

```sh
docker ps -a | grep eve-spai-br || echo "service gone OK"
curl -fsSI https://eve-spai.com/   # static site still 200
```

---

## 9. The fitting simulator (`/sim/` and `/api/sim/*`)

The same container serves a second trunk build, the fitting simulator, and keeps its doctrine
presets and each character's fit library in the same database (migration `0005_sim.sql` runs at
start).

- `deploy/web.sh <ssh target>` builds and uploads both browser apps (`wh-dist` and `sim-dist`)
  and restarts the container; `docker-compose.yml` mounts `./sim-dist:/srv/sim:ro`.
- nginx: the snippet gained `location /sim { proxy_pass http://eve-spai-br:8090; }` next to
  `/wh`; reload nginx after pasting it.
- `SIM_ADMIN_CHAR_IDS` (compose `environment:`): comma-separated character ids allowed to publish
  and delete doctrine presets. Everyone signed in sees the public presets; characters in The
  Imperium see the coalition's own and the enemy's.
- The fitting data: `deploy/sde-mirror.sh` bakes `dogma-v1.bin.gz` from the mirrored static-data
  zip with the `spai-fit-bake` binary inside the `eve-spai-br` image (so build the image before the
  first cron run) and publishes it under the static `/sde/` directory. The browser app fetches it
  from there; the desktop app falls back to it when a profile has no blob of its own.

Bake before anything asks for the blob: Cloudflare caches a 404 from the static site for four
hours, so a client that tried `/sde/dogma-v1.bin.gz` too early keeps getting it until the cache
expires or the URL is purged in the Cloudflare dashboard (Caching > Purge by URL).

**Check:**

```sh
curl -fsS https://eve-spai.com/sim/ | head -c 200          # the page
curl -fsSI https://eve-spai.com/sde/dogma-v1.bin.gz | head -1   # the data, after the first bake
curl -fsS https://eve-spai.com/api/sim/doctrines              # [] until an admin publishes
```

---

## 8. Release mirror (eve-spai.com/<os>/current)

Some pilots get poor download speeds from GitHub. `release-mirror.sh` copies the newest release's
files to the static site, where the app's updater can fetch them when GitHub is slow (only after
the user agreed to it, and always checked against GitHub's own SHA-256):

| Path | File |
|---|---|
| `/windows/current` | `eve-spai-windows-x86_64.exe` |
| `/macos/current` | `eve-spai-macos-aarch64` |
| `/linux/current` | `eve-spai-linux-x86_64` |

Each has a `.json` beside it with its `version` and `sha256`. The updater uses a mirrored file
only when that version matches the release it is installing.

Install the script and run it every 10 minutes, so the mirror is never more than about ten minutes
behind a release (it needs `curl`, `sha256sum` and `flock`, and does nothing when already current):

```sh
install -m 755 release-mirror.sh /srv/eve-spai/release-mirror.sh
/srv/eve-spai/release-mirror.sh /srv/nginx/www/eve-spai          # first copy, prints what it fetched
( crontab -l 2>/dev/null; echo "*/10 * * * * /srv/eve-spai/release-mirror.sh /srv/nginx/www/eve-spai >> /var/log/eve-spai-release-mirror.log 2>&1" ) | crontab -
```

Then paste `nginx-eve-spai-downloads.conf` into the eve-spai.com server block (it gives each file
its real name as a download and keeps it from being cached stale), and test and reload nginx as in
section 5.

**Check**:

```sh
curl -sI https://eve-spai.com/linux/current | grep -i -E 'content-disposition|content-length'
curl -s https://eve-spai.com/linux/current.json
cat /srv/nginx/www/eve-spai/release-mirror-checked.txt     # within the last 10 minutes
```
