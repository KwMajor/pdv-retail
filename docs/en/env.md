# Environment variables

*Versão em português: [../env.md](../env.md).*

All have development defaults; production requires its own values
(**never** commit `.env`).

| Variable | Default | Where | Use |
|---|---|---|---|
| `POSTGRES_USER` / `POSTGRES_PASSWORD` / `POSTGRES_DB` / `POSTGRES_PORT` | `pdv` / `pdv` / `pdv` / `5432` | compose + scripts | Local Postgres |
| `DATABASE_URL` | `postgres://pdv:pdv@localhost:5432/pdv` | API, sqlx, tests | Development database |
| `TEST_DATABASE_URL` | `.../pdv_test` | suite (`cargo test`) | Automated-tests database |
| `JWT_SECRET` | `dev-only-...` (insecure) | API | JWT HMAC — minimum 32 chars; boot aborts if weak; **missing with `APP_ENV=production` (or unset) = boot refused**; insecure fallback only in `development`/`test` |
| `APP_ENV` | `production` (fail-closed) | API | `development`/`test` enable the Swagger UI |
| `CORS_ORIGINS` | — (dev list) | API | Extra origins allowed by CORS beyond the default (localhost:1420, Tauri schemes) |
| `PORT` | `3000` | API | Cloud API HTTP port |
| `VITE_API_URL` | `http://localhost:3000` | frontend (build) | API base in the desktop/web app |
| `RUST_LOG` | `info` | API | `tracing` verbosity (e.g. `debug`) |
| `SKIP_SUDO` | `0` | setup scripts | `1` skips everything requiring sudo |

See `.env.example` (always mirrors this table).
