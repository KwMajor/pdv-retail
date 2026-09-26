#!/usr/bin/env bash
# Setup FRONTEND (Tauri v2 + React/TypeScript).
#
# Uso:
#   ./scripts/setup-frontend.sh            # interativo (pede senha sudo 1x)
#   ./scripts/setup-frontend.sh --yes      # não-interativo (CI / scripts)
#   SKIP_SUDO=1 ./scripts/setup-frontend.sh  # valida/pula tudo que exige sudo
#
# Instala/verifica (idempotente):
#   - via apt (sudo): libs do Tauri v2 (webkit2gtk, ayatana-appindicator,
#     librsvg2) + toolchain (pkg-config, build-essential, libssl-dev, unzip)
#   - user-local (sem sudo): fnm + Node 20 LTS
#   - projeto: `npm install` no desktop-client/ + `tsc --noEmit` de verificação
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/scripts/lib/common.sh"

for arg in "$@"; do
  case "$arg" in
    -h|--help)
      sed -n '2,14p' "${BASH_SOURCE[0]}" | sed 's/^# //; s/^#//'
      exit 0
      ;;
  esac
done
parse_common_flags "$@" || { echo "uso: $0 [--yes]" >&2; exit 1; }

# --- 0. sudo (1x) ------------------------------------------------------------
WITH_SUDO=1
setup_sudo || WITH_SUDO=0

# --- 1. Libs sistema do Tauri v2 ----------------------------------------------
if [ "$WITH_SUDO" = "1" ]; then
  TAURI_PKGS="pkg-config build-essential libssl-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev unzip"
  missing=()
  for pkg in $TAURI_PKGS; do
    dpkg -s "$pkg" >/dev/null 2>&1 || missing+=("$pkg")
  done
  if [ ${#missing[@]} -eq 0 ]; then
    ok "deps sistema OK (Tauri v2)"
  else
    info "instalando via apt: ${missing[*]}..."
    apt_update_once
    sudo apt-get install -y "${missing[@]}"
    ok "deps sistema OK"
  fi
else
  warn "puladas libs Tauri (SKIP_SUDO=1)."
fi

# --- 2. Node 20 LTS via fnm (user-local, sem sudo) -----------------------------
activate_node
if command -v node >/dev/null 2>&1; then
  ok "node OK ($(node --version), npm $(npm --version))"
else
  info "instalando fnm + node $NODE_VERSION (sem sudo)..."
  curl -fsSL https://fnm.vercel.app/install | bash -s -- --skip-shell
  activate_node
  # shellcheck disable=SC1090
  eval "$(fnm env --shell bash)"
  fnm install "$NODE_VERSION"
  fnm default "$NODE_VERSION"
  fnm use "$NODE_VERSION"
  ok "node OK ($(node --version), npm $(npm --version))"
  warn "adicione ao ~/.bashrc: export PATH=\"\$HOME/.local/share/fnm:\$HOME/.local/bin:\$PATH\" && eval \"\$(fnm env --use-on-cd --shell bash)\""
fi

# --- 3. Dependências npm do desktop-client --------------------------------------
if [ ! -f "$ROOT/desktop-client/package.json" ]; then
  warn "desktop-client/package.json ausente — pulando npm install."
else
  info "instalando dependências npm (desktop-client)..."
  ( cd "$ROOT/desktop-client" && npm install --no-audit --no-fund )
  ok "npm install OK"

  # --- 4. Typecheck (fecha o risco "TS nunca compilado") -----------------------
  info "verificando TypeScript (tsc --noEmit)..."
  ( cd "$ROOT/desktop-client" && ./node_modules/.bin/tsc --noEmit )
  ok "TypeScript OK (sem erros)"
fi

# --- Resumo ---------------------------------------------------------------------
echo ""
ok "resumo do frontend:"
command -v node >/dev/null 2>&1 && echo "  - node $(node --version) / npm $(npm --version)" || echo "  - node: AUSENTE"
[ -d "$ROOT/desktop-client/node_modules" ] && echo "  - desktop-client/node_modules: OK" || echo "  - desktop-client/node_modules: ausente"
