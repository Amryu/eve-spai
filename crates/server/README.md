# eve-spai-br — battle-report sharing API (Milestone 3)

A Linux-only axum/sqlx HTTP server that stores and serves shared EVE Spai battle
reports. JSON API only (the `/br` HTML viewer is Milestone 4).

## Workspace isolation

This crate is its **own** cargo workspace (note the empty `[workspace]` table in
`Cargo.toml`). It is deliberately NOT a member of the repo-root workspace, so the
desktop app's Windows/macOS CI (`cargo check --workspace`, `cargo build --release`)
never tries to cross-compile this tokio/axum/sqlx stack. It shares the battle model
with the app through `br-core`, pulled in by path. Always build/test it from inside
`crates/server`.

## Build & test (no database needed)

```
cd crates/server
cargo build            # offline, zero warnings, no DATABASE_URL required
cargo test             # unit tests (gzip-bomb, re-derivation, JWT, ids, ...)
```

The build uses runtime-checked sqlx (`sqlx::query`/`query_scalar`/`QueryBuilder`),
never the `query!` compile-time macros, so it needs neither a live database nor an
offline query cache.

## DB integration tests (gated)

The integration tests in `tests/integration.rs` are `#[ignore]`d and additionally
skip themselves when `DATABASE_URL` is unset, so a plain `cargo test` is green with
no Postgres. To run them against a throwaway database:

```
docker compose up -d        # or: podman compose up -d
export DATABASE_URL=postgres://evespai:evespai@localhost:5433/evespai
cargo test -- --ignored     # upload→fetch→list, unlisted, owner-only delete, dedupe, quota
```

Migrations in `migrations/` are embedded and applied automatically on startup (and
by the tests) via `sqlx::migrate!`.

## Running the server

```
export DATABASE_URL=postgres://evespai:evespai@localhost:5433/evespai
cargo run
```

## Endpoints

| Method | Path                 | Auth        | Notes                                            |
|--------|----------------------|-------------|--------------------------------------------------|
| GET    | `/healthz`           | —           | 200 "ok"                                         |
| POST   | `/api/session`       | EVE Bearer  | verify EVE SSO token once, mint OUR session token |
| POST   | `/api/br`            | Session     | gzipped `BattleReportDoc`; `?unlisted=true` opt  |
| GET    | `/api/br/{id}.json`  | —           | canonical stored doc; bumps `views`              |
| GET    | `/api/br`            | —           | public list; `system,from,to,participant,min_isk,sort,page` |
| GET    | `/api/br/mine`       | Session     | caller's reports, incl. unlisted                 |
| DELETE | `/api/br/{id}`       | Session     | owner-only (403 / 404)                           |

`POST /api/br` returns `{ "id", "url" }` (`url = <PUBLIC_BASE_URL>/br/<id>`). A re-upload
of the same document by the same character returns the existing id (200, idempotent).

### Session tokens

The privileged EVE SSO token carries write scopes and is audienced to EVE, so it is
accepted at exactly ONE route: `POST /api/session` (header `Authorization: Bearer
<EVE_JWT>`, no body). On success it returns 200 with:

```json
{ "token": "<our_jwt>", "expires_at": <unix_seconds>,
  "character_id": <i64>, "character_name": "<string>" }
```

`token` is OUR own HS256 JWT (`iss`/`aud` = `eve-spai.com`, `sub` = character id, `name`,
`exp`, `iat`), signed with `BR_SESSION_SECRET`. Every protected BR route
(`POST /api/br`, `GET /api/br/mine`, `DELETE /api/br/{id}`) authenticates with this
session token ONLY — a raw EVE token presented there is rejected with 401. The EVE token
is verified in memory at mint time and is never logged or persisted.

## Environment variables

| Variable               | Default                                   | Meaning                                  |
|------------------------|-------------------------------------------|------------------------------------------|
| `DATABASE_URL`         | *(required)*                              | Postgres connection string               |
| `BR_SESSION_SECRET`    | *(required)*                              | HS256 secret signing OUR session tokens  |
| `BR_SESSION_TTL_SECS`  | `86400` (24h)                             | lifetime of an issued session token      |
| `BIND_ADDR`            | `0.0.0.0:8080`                            | listen address                           |
| `EVE_CLIENT_ID`        | `fef96bde615b450bba89c9414962ca38`        | required `aud` value in the SSO token    |
| `EVE_JWKS_URL`         | `https://login.eveonline.com/oauth/jwks`  | JWKS source (cached, 1h TTL)             |
| `PUBLIC_BASE_URL`      | `https://eve-spai.com`                    | base for the returned report URL         |
| `BR_MAX_COMPRESSED`    | `1048576` (1 MiB)                         | max compressed upload (header + layer)   |
| `BR_MAX_DECOMPRESSED`  | `8388608` (8 MiB)                         | gzip-bomb ceiling on decompressed bytes  |
| `BR_MAX_PER_CHAR`      | `1000`                                    | lifetime reports per character           |
| `BR_UPLOADS_PER_HOUR`  | `60`                                      | uploads per rolling hour per character   |

## Token verification

Two distinct token layers (`src/auth.rs` for EVE, `src/session.rs` for ours):

**EVE SSO token (`/api/session` only).** Unlike the desktop app (which only
base64-decodes the payload), the server fully verifies the EVE access token: RS256
signature against EVE's JWKS, `iss` (`login.eveonline.com`), `exp`, and `aud` containing
`EVE_CLIENT_ID`. The identity (`uploader_char_id`, `uploader_name`) comes from `sub`
(`CHARACTER:EVE:<id>`) and `name`. The `Verifier` is constructed either live (cached
JWKS) or from an injected `JwkSet` — the latter is the seam the JWT unit tests use to
verify tokens with a locally generated keypair, no network.

**Session token (every protected BR route).** `SessionIssuer` signs an HS256 JWT
(`iss`/`aud` = `eve-spai.com`) at mint time; `SessionVerifier` / the `SessionIdentity`
extractor validate the signature, `iss`, `aud`, and `exp` and yield the same `Identity`
the handlers already use. The EVE token never reaches these routes.
