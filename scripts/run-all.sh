#!/usr/bin/env bash
# Sobe o projeto COMPLETO localmente: Postgres + Cloud API + frontend.
#
# Uso:
#   ./scripts/run-all.sh              # tudo (postgres + api + web vite)
#   ./scripts/run-all.sh --api-only   # só postgres + api
#   ./scripts/run-all.sh --web-only   # só postgres + web (pressupõe api no ar)
#   ./scripts/run-all.sh --tauri      # janela nativa (npx tauri dev) em vez do vite
#
# Ctrl+C derruba API e frontend; o Postgres continua no ar
# (pare com `docker compose down`). Requer `./scripts/setup-*.sh` já executado.
# Rode em foreground: o trap de limpeza atende INT (Ctrl+C) e TERM.
# Supervisores (systemd/docker/CI) devem parar via SIGTERM.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
API_PORT="${PORT:-3000}"
WEB_PORT=1420

MODE="all"
for arg in "$@"; do
  case "$arg" in
    --api-only) MODE="api" ;;
    --web-only) MODE="web" ;;
    --tauri) MODE="tauri" ;;
    -h|--help)
      sed -n '2,12p' "${BASH_SOURCE[0]}" | sed 's/^# //; s/^#//'
      exit 0
      ;;
    *) echo "flag desconhecida: $arg" >&2; exit 1 ;;
  esac
done

info() { printf '\033[1;34m[run]\033[0m %s\n' "$*"; }
ok() { printf '\033[1;32m[run]\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m[run]\033[0m %s\n' "$*" >&2; }

# --- 0. pré-requisitos ---------------------------------------------------------
export PATH="$HOME/.cargo/bin:$PATH"
command -v docker >/dev/null 2>&1 || { fail "docker ausente — rode scripts/setup-backend.sh"; exit 1; }
command -v cargo >/dev/null 2>&1 || { fail "cargo ausente — rode scripts/setup-backend.sh"; exit 1; }
command -v sqlx >/dev/null 2>&1 || { fail "sqlx ausente — rode scripts/setup-backend.sh"; exit 1; }
export PATH="$HOME/.local/share/fnm:$HOME/.local/bin:$PATH"
if [ -x "$HOME/.local/share/fnm/fnm" ]; then
  # shellcheck disable=SC1090
  eval "$($HOME/.local/share/fnm/fnm env --shell bash)" 2>/dev/null || true
fi
if [ "$MODE" != "api" ] && ! command -v node >/dev/null 2>&1; then
  fail "node ausente — rode scripts/setup-frontend.sh"; exit 1
fi
[ -f "$ROOT/.env" ] || { fail ".env ausente — rode scripts/setup-backend.sh"; exit 1; }
for port in "$API_PORT" "$WEB_PORT"; do
  if (command -v ss >/dev/null 2>&1 && ss -ltn 2>/dev/null | grep -q ":$port ") \
    || (exec 3<>/dev/tcp/127.0.0.1/$port) 2>/dev/null; then
    exec 3>&- 2>/dev/null || true
    fail "porta $port ocupada — pare o processo antes."
    exit 1
  fi
done

# --- 1. postgres ---------------------------------------------------------------
info "subindo postgres..."
docker compose -f "$ROOT/docker-compose.yml" up -d >/dev/null
info "aguardando postgres saudável..."
for _ in $(seq 1 30); do
  if docker exec pdv-postgres pg_isready -U "${POSTGRES_USER:-pdv}" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
docker exec pdv-postgres pg_isready -U "${POSTGRES_USER:-pdv}" >/dev/null \
  || { fail "postgres não ficou pronto"; exit 1; }
ok "postgres no ar"

# --- 2. migrations --------------------------------------------------------------
info "aplicando migrations..."
# shellcheck disable=SC1090
set -a; source "$ROOT/.env"; set +a
export DATABASE_URL="${DATABASE_URL:?DATABASE_URL ausente no .env}"
( cd "$ROOT/cloud-api" && sqlx migrate run )
ok "migrations aplicadas"

# --- 3. processos (foreground, Ctrl+C limpa) --------------------------------------
PIDS=""
# Mata um PID e TODOS os descendentes (ex: `npm run dev` orfana o vite se
# só o npm receber o sinal). TERM educado primeiro, KILL de reforço depois.
killtree() {
  local pid="$1" child
  for child in $(ps -o pid= --ppid "$pid" 2>/dev/null); do
    killtree "$child"
  done
  kill "$pid" 2>/dev/null || true
}
killtree9() {
  local pid="$1" child
  for child in $(ps -o pid= --ppid "$pid" 2>/dev/null); do
    killtree9 "$child"
  done
  kill -9 "$pid" 2>/dev/null || true
}
cleanup() {
  info "parando api/frontend (postgres continua no ar)..."
  # shellcheck disable=SC2086
  for pid in $PIDS; do killtree "$pid"; done
  sleep 3
  for pid in $PIDS; do killtree9 "$pid"; done
  wait 2>/dev/null || true
}
trap cleanup INT TERM EXIT

if [ "$MODE" != "web" ]; then
  info "iniciando Cloud API (:$API_PORT)..."
  ( cd "$ROOT/cloud-api" && exec cargo run -q ) &
  PIDS="$PIDS $!"
  info "aguardando /health..."
  for _ in $(seq 1 60); do
    if curl -fsS "http://127.0.0.1:$API_PORT/health" >/dev/null 2>&1; then
      break
    fi
    sleep 1
  done
  curl -fsS "http://127.0.0.1:$API_PORT/health" >/dev/null \
    || { fail "API não respondeu /health"; exit 1; }
  ok "API no ar → http://localhost:$API_PORT (docs: /docs)"
fi

if [ "$MODE" != "api" ]; then
  if [ "$MODE" = "tauri" ]; then
    info "abrindo janela Tauri (npx tauri dev)..."
    ( cd "$ROOT/desktop-client" && exec npx tauri dev ) &
  else
    info "iniciando web (:$WEB_PORT)..."
    ( cd "$ROOT/desktop-client" && exec npm run dev -- --port "$WEB_PORT" --strictPort ) &
  fi
  PIDS="$PIDS $!"
  ok "frontend no ar → http://localhost:$WEB_PORT"
fi

ok "tudo no ar. Ctrl+C para parar (postgres segue com 'docker compose down')."
wait
