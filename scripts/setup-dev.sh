#!/usr/bin/env bash
# Instalador dev PDV SaaS — time 100% Linux (Mint / Ubuntu noble).
# Wrapper fino: executa o setup do backend E do frontend em sequência.
# (Lógica real mora em setup-backend.sh / setup-frontend.sh + lib/common.sh.)
#
# Uso (qualquer dev do time):
#   ./scripts/setup-dev.sh            # interativo (pede senha sudo 1x por etapa)
#   ./scripts/setup-dev.sh --yes      # não-interativo (CI / scripts)
#   SKIP_SUDO=1 ./scripts/setup-dev.sh  # valida/pula tudo que exige sudo
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

"$ROOT/scripts/setup-backend.sh" "$@"
"$ROOT/scripts/setup-frontend.sh" "$@"

printf '\033[1;32m[setup]\033[0m pronto. Ambiente backend + frontend verificado.\n'
