# Running locally

*Versão em português: [../run-local.md](../run-local.md).*

## Prerequisites (once per machine)

100% Linux team (Mint/Ubuntu). Two idempotent scripts:

```bash
./scripts/setup-backend.sh   # Docker, Postgres client, Rust, sqlx-cli, .env, compose up
./scripts/setup-frontend.sh  # Tauri libs, Node 20 (fnm), npm install, tsc
# or everything at once:
./scripts/setup-dev.sh
```

Flags: `--yes` (non-interactive), `SKIP_SUDO=1` (skips apt/docker).

## Bringing it all up

```bash
./scripts/run-all.sh              # postgres + API (:3000) + web (:1420)
./scripts/run-all.sh --api-only   # postgres + API only
./scripts/run-all.sh --web-only   # postgres + web only (API already up)
./scripts/run-all.sh --tauri      # native window instead of browser
```

Ctrl+C stops API and frontend; Postgres keeps running (`docker compose down` to stop everything).

URLs: API `http://localhost:3000`, Swagger `http://localhost:3000/docs` (with `APP_ENV=development`), web `http://localhost:1420`.

## First session (demo seed)

The database starts empty — no store/user means no login:

```bash
cd cloud-api && cargo run --example seed_dev
```

Creates the Demo Store + manager + 4 products and **prints the credentials** (store UUID, email, password) to paste into the login screen. Idempotent (safe to re-run). **Never** with `APP_ENV=production` (the seed refuses).

## Troubleshooting

| Symptom | Likely cause |
|---|---|
| `porta 3000/1420 ocupada` (port busy) | another process; stop it first (`lsof -i :3000`) |
| `.env ausente` (missing .env) | run `setup-backend.sh` (creates from `.env.example`) |
| `postgres não ficou pronto` (not ready) | `docker compose ps`; `docker compose logs postgres` |
| `tauri dev` with no window | no graphical session (`DISPLAY`); use web mode |
| API 401 on login | check `X-Store-ID` (seed store UUID) and password |
