#!/usr/bin/env bash
# Portão local do CI (AGENTS.md §8): espelha os jobs de `.github/workflows/ci.yml`.
# OBRIGATÓRIO verde antes de todo `git push` (o hook pre-push chama daqui).
# Falha no primeiro gate vermelho, ecoando o comando exato para reproduzir.
#
# Uso: ./scripts/check-all.sh
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

step() { printf '\033[1;34m[check]\033[0m %s\n' "$*"; }
ok() { printf '\033[1;32m[check]\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m[check]\033[0m %s\n' "$*" >&2; exit 1; }

export PATH="$HOME/.cargo/bin:$PATH"
export PATH="$HOME/.local/share/fnm:$HOME/.local/bin:$PATH"
if [ -x "$HOME/.local/share/fnm/fnm" ]; then
  # shellcheck disable=SC1090
  eval "$($HOME/.local/share/fnm/fnm env --shell bash)" 2>/dev/null || true
fi

command -v cargo >/dev/null 2>&1 || fail "cargo ausente — rode scripts/setup-backend.sh"
command -v node >/dev/null 2>&1 || fail "node ausente — rode scripts/setup-frontend.sh"

# --- backend: fmt + clippy ------------------------------------------------------
step "cargo fmt --check  (job: fmt-clippy)"
( cd "$ROOT/cloud-api" && cargo fmt --all --check ) || fail "rode: cargo fmt --all (em cloud-api/)"
ok "fmt"

step "cargo clippy -D warnings  (job: fmt-clippy)"
( cd "$ROOT/cloud-api" && cargo clippy --all-targets --locked -- -D warnings ) || fail "clippy acusou"
ok "clippy"

# --- backend: testes + prepare (job: backend-test) -------------------------------
step "postgres + migrate + cargo test"
docker compose -f "$ROOT/docker-compose.yml" up -d >/dev/null
for _ in $(seq 1 30); do
  docker exec pdv-postgres pg_isready -U "${POSTGRES_USER:-pdv}" >/dev/null 2>&1 && break
  sleep 1
done
# shellcheck disable=SC1090
set -a; source "$ROOT/.env"; set +a
export DATABASE_URL="${DATABASE_URL:?DATABASE_URL ausente no .env}"
export TEST_DATABASE_URL="${TEST_DATABASE_URL:-postgres://pdv:pdv@localhost:5432/pdv_test}"
export JWT_SECRET="${JWT_SECRET:?JWT_SECRET ausente no .env}"
export APP_ENV=test
( cd "$ROOT/cloud-api" && sqlx migrate run ) || fail "migrate quebrou"
( cd "$ROOT/cloud-api" && DATABASE_URL="$DATABASE_URL" cargo test --locked ) || fail "cargo test quebrou"
( cd "$ROOT/cloud-api" && cargo sqlx prepare --check ) || fail "sqlx prepare --check: rode cargo sqlx prepare"
ok "backend testes + prepare"

# --- supply chain (job: supply-chain) ---------------------------------------------
step "cargo-deny"
if command -v cargo-deny >/dev/null 2>&1; then
  ( cd "$ROOT" && cargo deny --manifest-path cloud-api/Cargo.toml check ) || fail "cargo-deny acusou"
  ok "cargo-deny"
else
  echo "[check] AVISO: cargo-deny ausente (cargo install cargo-deny) — pulando gate local, CI barra"
fi

# --- semgrep (job: semgrep, bloqueante) ----------------------------------------------
step "semgrep (regras próprias + packs)"
docker run --rm -v "$ROOT:/src" -w /src --network host -e SEMGREP_SEND_METRICS=off \
  semgrep/semgrep semgrep scan \
  --config p/rust --config p/typescript --config p/javascript --config p/secrets \
  --config p/supply-chain --config p/dockerfile --config p/github-actions \
  --config .semgrep/rules --exclude target --exclude node_modules \
  || fail "semgrep acusou (ver achados acima)"
ok "semgrep"

# --- frontend (job: frontend) ----------------------------------------------------------
step "tsc + eslint + vitest + vite build + npm audit"
( cd "$ROOT/desktop-client" && ./node_modules/.bin/tsc --noEmit ) || fail "tsc acusou"
( cd "$ROOT/desktop-client" && ./node_modules/.bin/eslint src/ ) || fail "eslint acusou"
( cd "$ROOT/desktop-client" && npm test ) || fail "vitest acusou"
( cd "$ROOT/desktop-client" && npm run build ) || fail "vite build quebrou"
( cd "$ROOT/desktop-client" && npm audit --omit=dev --audit-level=moderate ) || fail "npm audit acusou"
ok "frontend"

echo ""
ok "check-all VERDE — pode dar push."
