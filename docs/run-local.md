# Rodando localmente

## Pré-requisitos (uma vez por máquina)

Time 100% Linux (Mint/Ubuntu). Dois scripts idempotentes:

```bash
./scripts/setup-backend.sh   # Docker, Postgres client, Rust, sqlx-cli, .env, compose up
./scripts/setup-frontend.sh  # libs Tauri, Node 20 (fnm), npm install, tsc
# ou tudo de uma vez:
./scripts/setup-dev.sh
```

Flags: `--yes` (não-interativo), `SKIP_SUDO=1` (pula apt/docker).

## Subindo tudo

```bash
./scripts/run-all.sh              # postgres + API (:3000) + web (:1420)
./scripts/run-all.sh --api-only   # só postgres + API
./scripts/run-all.sh --web-only   # só postgres + web (API já no ar)
./scripts/run-all.sh --tauri      # janela nativa em vez do navegador
```

Ctrl+C derruba API e frontend; o Postgres continua no ar (`docker compose down` para parar tudo).

URLs: API `http://localhost:3000`, Swagger `http://localhost:3000/docs` (com `APP_ENV=development`), web `http://localhost:1420`.

## Primeira sessão (seed demo)

O banco nasce vazio — sem loja/usuário não há login:

```bash
cd cloud-api && cargo run --example seed_dev
```

Cria a Loja Demo + gerente + 4 produtos e **imprime as credenciais** (loja UUID, email, senha) para colar na tela de login. Idempotente (pode rodar de novo). **Nunca** com `APP_ENV=production` (o seed recusa).

## Solução de problemas

| Sintoma | Causa provável |
|---|---|
| `porta 3000/1420 ocupada` | outro processo; pare antes (`lsof -i :3000`) |
| `.env ausente` | rode `setup-backend.sh` (cria do `.env.example`) |
| `postgres não ficou pronto` | `docker compose ps`; `docker compose logs postgres` |
| `tauri dev` sem janela | sem sessão gráfica (`DISPLAY`); use o modo web |
| API 401 no login | confira `X-Store-ID` (UUID da loja do seed) e senha |
