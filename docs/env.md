# Variáveis de ambiente

*English version: [en/env.md](en/env.md).*

Todas têm default de desenvolvimento; produção exige valores próprios
(**nunca** commitar `.env`).

| Variável | Default | Onde vale | Uso |
|---|---|---|---|
| `POSTGRES_USER` / `POSTGRES_PASSWORD` / `POSTGRES_DB` / `POSTGRES_PORT` | `pdv` / `pdv` / `pdv` / `5432` | compose + scripts | Postgres local |
| `DATABASE_URL` | `postgres://pdv:pdv@localhost:5432/pdv` | API, sqlx, testes | Banco de desenvolvimento |
| `TEST_DATABASE_URL` | `.../pdv_test` | suíte (`cargo test`) | Banco dos testes automatizados |
| `JWT_SECRET` | `dev-only-...` (inseguro) | API | HMAC do JWT — mínimo 32 chars; boot aborta se fraco; **produção exige segredo forte** |
| `APP_ENV` | `production` (fail-closed) | API | `development`/`test` ligam Swagger UI |
| `CORS_ORIGINS` | — (lista dev) | API | Origens extras liberadas no CORS além do padrão (localhost:1420, esquemas Tauri) |
| `PORT` | `3000` | API | Porta HTTP da Cloud API |
| `VITE_API_URL` | `http://localhost:3000` | frontend (build) | Base da API no app desktop/web |
| `RUST_LOG` | `info` | API | Verbosidade do `tracing` (ex: `debug`) |
| `SKIP_SUDO` | `0` | scripts setup | `1` pula tudo que exige sudo |

Ver `.env.example` (sempre espelha esta tabela).
