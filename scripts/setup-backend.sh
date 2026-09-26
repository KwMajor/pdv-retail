#!/usr/bin/env bash
# Setup BACKEND (Cloud API Rust + Postgres via Docker).
#
# Uso:
#   ./scripts/setup-backend.sh            # interativo (pede senha sudo 1x)
#   ./scripts/setup-backend.sh --yes      # não-interativo (CI / scripts)
#   SKIP_SUDO=1 ./scripts/setup-backend.sh  # valida/pula tudo que exige sudo
#
# Instala/verifica (idempotente):
#   - via apt (sudo): ca-certificates, curl, gnupg, unzip, pkg-config,
#     build-essential (linker p/ compilar Rust), postgresql-client-16,
#     Docker Engine + compose plugin
#   - user-local (sem sudo): Rust via rustup, sqlx-cli
#   - projeto: .env a partir de .env.example + `docker compose up -d`
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/scripts/lib/common.sh"

for arg in "$@"; do
  case "$arg" in
    -h|--help)
      sed -n '2,15p' "${BASH_SOURCE[0]}" | sed 's/^# //; s/^#//'
      exit 0
      ;;
  esac
done
parse_common_flags "$@" || { echo "uso: $0 [--yes]" >&2; exit 1; }

# --- 0. sudo (1x) ------------------------------------------------------------
WITH_SUDO=1
setup_sudo || WITH_SUDO=0

# --- 1. Docker ---------------------------------------------------------------
if [ "$WITH_SUDO" = "1" ]; then
  UBUNTU_CODENAME="$(grep -E '^UBUNTU_CODENAME=' /etc/os-release 2>/dev/null | cut -d= -f2 || true)"
  [ -n "$UBUNTU_CODENAME" ] || UBUNTU_CODENAME="noble"

  if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
    ok "docker OK ($(docker --version))"
  else
    info "instalando Docker Engine + compose plugin (repo oficial, $UBUNTU_CODENAME)..."
    apt_update_once
    sudo apt-get install -y ca-certificates curl gnupg
    sudo install -m 0755 -d /etc/apt/keyrings
    if [ ! -f /etc/apt/keyrings/docker.gpg ]; then
      curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
      sudo chmod a+r /etc/apt/keyrings/docker.gpg
    fi
    if [ ! -f /etc/apt/sources.list.d/docker.list ]; then
      echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu $UBUNTU_CODENAME stable" \
        | sudo tee /etc/apt/sources.list.d/docker.list >/dev/null
      DOCKER_LIST_JUST_CREATED=1
    fi
    # O repo acabou de entrar (ou pode ter entrado numa run anterior que
    # falhou antes do install): força update para o apt enxergar o docker-ce.
    if [ "${DOCKER_LIST_JUST_CREATED:-0}" = "1" ] || ! apt-cache policy docker-ce 2>/dev/null | grep -q download.docker.com; then
      info "atualizando índice apt com o repo Docker..."
      sudo apt-get update
      APT_UPDATED=1
    else
      apt_update_once
    fi
    sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
    ok "docker OK ($(docker --version))"
  fi

  # `set -u` exige default: USER pode não existir em shells mínimos/CI.
  TARGET_USER="${USER:-$(id -un)}"
  if groups "$TARGET_USER" | grep -qw docker; then
    ok "usuário já está no grupo docker"
  else
    info "adicionando $TARGET_USER ao grupo docker..."
    sudo usermod -aG docker "$TARGET_USER"
    warn "grupo docker adicionado — faça logout/login (ou 'newgrp docker') para usar docker sem sudo."
  fi

  # --- 2. Deps sistema do backend: toolchain C + psql client ------------------
  BACKEND_PKGS="pkg-config build-essential libssl-dev unzip postgresql-client-16"
  missing=()
  for pkg in $BACKEND_PKGS; do
    dpkg -s "$pkg" >/dev/null 2>&1 || missing+=("$pkg")
  done
  if [ ${#missing[@]} -eq 0 ]; then
    ok "deps sistema OK (toolchain + psql client)"
  else
    info "instalando via apt: ${missing[*]}..."
    apt_update_once
    sudo apt-get install -y "${missing[@]}"
    ok "deps sistema OK"
  fi
else
  warn "pulada etapa apt/docker (SKIP_SUDO=1)."
fi

# --- 3. Rust via rustup (user-local, sem sudo) --------------------------------
if command -v cargo >/dev/null 2>&1; then
  ok "cargo OK ($(cargo --version))"
else
  info "instalando Rust via rustup (sem sudo)..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  # shellcheck disable=SC1090
  [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
  ok "cargo OK ($(cargo --version))"
fi

# --- 4. sqlx-cli (user-local via cargo, sem sudo) -----------------------------
export PATH="$HOME/.cargo/bin:$PATH"
if command -v sqlx >/dev/null 2>&1; then
  ok "sqlx-cli OK ($(sqlx --version))"
else
  info "instalando sqlx-cli (postgres only, pode levar alguns minutos)..."
  cargo install sqlx-cli --no-default-features --features postgres
  ok "sqlx-cli OK ($(sqlx --version))"
fi

# --- 5. .env ------------------------------------------------------------------
if [ ! -f "$ROOT/.env" ]; then
  cp "$ROOT/.env.example" "$ROOT/.env"
  ok ".env criado a partir de .env.example"
else
  ok ".env já existe (mantido)"
fi

# --- 6. Postgres via compose ---------------------------------------------------
compose() {
  if docker compose version >/dev/null 2>&1; then
    docker compose "$@"
  else
    warn "docker sem permissão direta, tentando com sudo..."
    sudo docker compose "$@"
  fi
}

if [ "$WITH_SUDO" = "0" ] && ! command -v docker >/dev/null 2>&1; then
  warn "pulei 'compose up -d' (sem docker + SKIP_SUDO=1)."
elif command -v docker >/dev/null 2>&1; then
  info "subindo postgres..."
  compose -f "$ROOT/docker-compose.yml" up -d
  compose -f "$ROOT/docker-compose.yml" ps
  ok "postgres no ar."
else
  warn "docker ausente — rode o script de novo após o login no grupo docker."
fi

# --- Resumo -------------------------------------------------------------------
echo ""
ok "resumo do backend:"
command -v docker >/dev/null 2>&1 && echo "  - $(docker --version) / $(docker compose version)" || echo "  - docker: AUSENTE (rode de novo após login no grupo)"
command -v cargo >/dev/null 2>&1 && echo "  - $(cargo --version)" || echo "  - cargo: AUSENTE"
command -v sqlx >/dev/null 2>&1 && echo "  - $(sqlx --version)" || echo "  - sqlx: AUSENTE"
command -v psql >/dev/null 2>&1 && echo "  - $(psql --version)" || echo "  - psql: ausente (ok se SKIP_SUDO=1)"
