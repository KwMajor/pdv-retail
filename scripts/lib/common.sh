#!/usr/bin/env bash
# Biblioteca comum dos setups (backend/frontend). NÃO executar direto:
#   source "$ROOT/scripts/lib/common.sh"
# Idempotente. Time 100% Linux (Mint / Ubuntu noble).
#
# O que centraliza:
#   - log helpers (info/warn/ok/fail), parse de --yes, keepalive do sudo,
#     apt_update_once, ativação do Node via fnm, versões pinadas.

# --- versões pinadas (um só lugar) -------------------------------------------
NODE_VERSION="20.19.0"

# --- flags -------------------------------------------------------------------
# NOTA HONESTA: installs apt já rodam com `-y` sempre; `--yes` existe por
# compatibilidade (CI/scripts) e como reserva para prompts futuros.
ASSUME_YES=0
parse_common_flags() {
  for arg in "$@"; do
    case "$arg" in
      -y|--yes) ASSUME_YES=1 ;;
      -h|--help) return 2 ;;
      *) echo "flag desconhecida: $arg (use --yes)" >&2; return 1 ;;
    esac
  done
  return 0
}

# --- log ---------------------------------------------------------------------
# LOG_TAG permite ao run-all.sh assinar como [run] em vez de [setup].
info() { printf '\033[1;34m[%s]\033[0m %s\n' "${LOG_TAG:-setup}" "$*"; }
warn() { printf '\033[1;33m[%s]\033[0m %s\n' "${LOG_TAG:-setup}" "$*"; }
ok() { printf '\033[1;32m[%s]\033[0m %s\n' "${LOG_TAG:-setup}" "$*"; }
fail() { printf '\033[1;31m[%s]\033[0m %s\n' "${LOG_TAG:-setup}" "$*" >&2; }

# --- sudo (pede senha 1x, mantém vivo em installs longos) ---------------------
SUDO_KEEPALIVE_PID=""
setup_sudo() {
  SKIP_SUDO="${SKIP_SUDO:-0}"
  if [ "$SKIP_SUDO" = "1" ]; then
    warn "SKIP_SUDO=1 — pulando tudo que exige sudo."
    return 1
  fi
  info "validando sudo (vai pedir sua senha 1x se necessário)..."
  sudo -v || { fail "sudo indisponível. Rode com um usuário sudoer."; return 1; }
  ( while true; do sudo -n true 2>/dev/null || break; sleep 50; done ) &
  SUDO_KEEPALIVE_PID=$!
  trap 'kill "$SUDO_KEEPALIVE_PID" 2>/dev/null || true' EXIT
  return 0
}

# --- apt (update uma única vez por execução) ----------------------------------
APT_UPDATED=0
apt_update_once() {
  if [ "$APT_UPDATED" -eq 0 ]; then
    info "apt-get update..."
    sudo apt-get update
    APT_UPDATED=1
  fi
}

# --- Node via fnm (user-local, sem sudo) --------------------------------------
activate_node() {
  export FNM_DIR="${FNM_DIR:-$HOME/.local/share/fnm}"
  export PATH="$FNM_DIR:$HOME/.local/bin:$PATH"
  # fnm deixa o node fora do PATH até o eval — ativa primeiro se já instalado
  if [ -x "$FNM_DIR/fnm" ]; then
    # shellcheck disable=SC1090
    eval "$("$FNM_DIR/fnm" env --shell bash)" 2>/dev/null || true
  fi
}
