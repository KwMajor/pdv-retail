#!/usr/bin/env bash
# Instalador dev PDV SaaS — time 100% Linux (Mint / Ubuntu noble).
# Instala TUDO que o projeto precisa em um comando. Idempotente.
#
# Uso (qualquer dev do time):
#   ./scripts/setup-dev.sh            # interativo (pede senha sudo 1x)
#   ./scripts/setup-dev.sh --yes      # não-interativo (CI / scripts)
#   SKIP_SUDO=1 ./scripts/setup-dev.sh  # valida/pula tudo que exige sudo
#
# O que instala:
#   - via apt (sudo): ca-certificates, curl, gnupg, unzip, pkg-config,
#     build-essential, libssl-dev, libwebkit2gtk-4.1-dev,
#     libayatana-appindicator3-dev, librsvg2-dev (Tauri v2),
#     postgresql-client-16, Docker Engine + compose plugin
#   - user-local (sem sudo): fnm + Node 20 LTS, Rust via rustup, sqlx-cli
#   - projeto: .env a partir de .env.example + `docker compose up -d`
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NODE_VERSION="20.19.0"

ASSUME_YES=0
for arg in "$@"; do
  case "$arg" in
    -y|--yes) ASSUME_YES=1 ;;
    -h|--help)
      sed -n '2,20p' "${BASH_SOURCE[0]}" | sed 's/^# //; s/^#//'
      exit 0
      ;;
    *) echo "flag desconhecida: $arg (use --yes)" >&2; exit 1 ;;
  esac
done

info() { printf '\033[1;34m[setup]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[setup]\033[0m %s\n' "$*"; }
ok() { printf '\033[1;32m[setup]\033[0m %s\n' "$*"; }
fail() { printf '\033[1;31m[setup]\033[0m %s\n' "$*" >&2; }

APT_UPDATED=0
apt_update_once() {
  if [ "$APT_UPDATED" -eq 0 ]; then
    info "apt-get update..."
    sudo apt-get update
    APT_UPDATED=1
  fi
}

# --- 0. sudo (1x) ------------------------------------------------------------
SKIP_SUDO="${SKIP_SUDO:-0}"
if [ "$SKIP_SUDO" = "1" ]; then
  warn "SKIP_SUDO=1 — pulando tudo que exige sudo (docker, libs Tauri, psql)."
else
  info "validando sudo (vai pedir sua senha 1x se necessário)..."
  sudo -v || { fail "sudo indisponível. Rode com um usuário sudoer."; exit 1; }
  # mantém o timestamp do sudo vivo durante installs longos (cargo, etc.)
  ( while true; do sudo -n true 2>/dev/null || break; sleep 50; done ) &
  SUDO_KEEPALIVE_PID=$!
  trap 'kill "$SUDO_KEEPALIVE_PID" 2>/dev/null || true' EXIT
fi

# --- 1. Base apt + Docker ----------------------------------------------------
if [ "$SKIP_SUDO" != "1" ]; then
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

  if groups "$USER" | grep -qw docker; then
    ok "usuário já está no grupo docker"
  else
    info "adicionando $USER ao grupo docker..."
    sudo usermod -aG docker "$USER"
    warn "grupo docker adicionado — faça logout/login (ou 'newgrp docker') para usar docker sem sudo."
  fi

  # --- 2. Deps sistema: Tauri v2 + build + psql client -----------------------
  TAURI_PKGS="pkg-config build-essential libssl-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev unzip"
  missing=()
  for pkg in $TAURI_PKGS postgresql-client-16; do
    dpkg -s "$pkg" >/dev/null 2>&1 || missing+=("$pkg")
  done
  if [ ${#missing[@]} -eq 0 ]; then
    ok "deps sistema OK (Tauri v2 + psql client)"
  else
    info "instalando via apt: ${missing[*]}..."
    apt_update_once
    sudo apt-get install -y "${missing[@]}"
    ok "deps sistema OK"
  fi
else
  warn "pulada etapa apt/docker (SKIP_SUDO=1)."
fi

# --- 3. Node 20 LTS via fnm (user-local, sem sudo) ---------------------------
export FNM_DIR="${FNM_DIR:-$HOME/.local/share/fnm}"
export PATH="$FNM_DIR:$HOME/.local/bin:$PATH"
# fnm deixa o node fora do PATH até o eval — ativa primeiro se já instalado
if [ -x "$FNM_DIR/fnm" ]; then
  # shellcheck disable=SC1090
  eval "$("$FNM_DIR/fnm" env --shell bash)" 2>/dev/null || true
fi
if command -v node >/dev/null 2>&1; then
  ok "node OK ($(node --version), npm $(npm --version))"
else
  info "instalando fnm + node $NODE_VERSION (sem sudo)..."
  curl -fsSL https://fnm.vercel.app/install | bash -s -- --skip-shell
  export PATH="$FNM_DIR:$HOME/.local/bin:$PATH"
  # shellcheck disable=SC1090
  eval "$(fnm env --shell bash)"
  fnm install "$NODE_VERSION"
  fnm default "$NODE_VERSION"
  fnm use "$NODE_VERSION"
  ok "node OK ($(node --version), npm $(npm --version))"
  warn "adicione ao ~/.bashrc: export PATH=\"\$HOME/.local/share/fnm:\$HOME/.local/bin:\$PATH\" && eval \"\$(fnm env --use-on-cd --shell bash)\""
fi

# --- 4. Rust via rustup (user-local, sem sudo) --------------------------------
if command -v cargo >/dev/null 2>&1; then
  ok "cargo OK ($(cargo --version))"
else
  info "instalando Rust via rustup (sem sudo)..."
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
  # shellcheck disable=SC1090
  [ -f "$HOME/.cargo/env" ] && source "$HOME/.cargo/env"
  ok "cargo OK ($(cargo --version))"
fi

# --- 5. sqlx-cli (user-local via cargo, sem sudo) -----------------------------
export PATH="$HOME/.cargo/bin:$PATH"
if command -v sqlx >/dev/null 2>&1; then
  ok "sqlx-cli OK ($(sqlx --version))"
else
  info "instalando sqlx-cli (postgres only, pode levar alguns minutos)..."
  cargo install sqlx-cli --no-default-features --features postgres
  ok "sqlx-cli OK ($(sqlx --version))"
fi

# --- 6. .env ------------------------------------------------------------------
if [ ! -f "$ROOT/.env" ]; then
  cp "$ROOT/.env.example" "$ROOT/.env"
  ok ".env criado a partir de .env.example"
else
  ok ".env já existe (mantido)"
fi

# --- 7. Postgres via compose ---------------------------------------------------
compose() {
  if docker compose version >/dev/null 2>&1; then
    docker compose "$@"
  else
    warn "docker sem permissão direta, tentando com sudo..."
    sudo docker compose "$@"
  fi
}

if [ "$SKIP_SUDO" = "1" ] && ! command -v docker >/dev/null 2>&1; then
  warn "pulei 'compose up -d' (sem docker + SKIP_SUDO=1)."
elif command -v docker >/dev/null 2>&1; then
  info "subindo postgres..."
  compose -f "$ROOT/docker-compose.yml" up -d
  compose -f "$ROOT/docker-compose.yml" ps
  ok "postgres no ar. Checagem: sqlx database setup deve conectar em \$DATABASE_URL"
else
  warn "docker ausente — rode o script de novo após o login no grupo docker."
fi

# --- Resumo -------------------------------------------------------------------
echo ""
ok "resumo do ambiente:"
command -v docker >/dev/null 2>&1 && echo "  - $(docker --version) / $(docker compose version)" || echo "  - docker: AUSENTE (rode de novo após login no grupo)"
command -v node >/dev/null 2>&1 && echo "  - node $(node --version) / npm $(npm --version)" || echo "  - node: AUSENTE"
command -v cargo >/dev/null 2>&1 && echo "  - $(cargo --version)" || echo "  - cargo: AUSENTE"
command -v sqlx >/dev/null 2>&1 && echo "  - $(sqlx --version)" || echo "  - sqlx: AUSENTE"
command -v psql >/dev/null 2>&1 && echo "  - $(psql --version)" || echo "  - psql: ausente (ok se SKIP_SUDO=1)"
ok "pronto. Próximo: docker compose up -d && sqlx database setup (quando cloud-api existir)."
