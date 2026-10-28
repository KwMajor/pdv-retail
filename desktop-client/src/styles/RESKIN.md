# Re-skin e whitelabel

**Regra:** loja não edita CSS. A marca da loja é configuração em runtime (nome, logo, preset e destaque) na tela **Configurações → Marca da loja** (US-23). O build do reseller é definido em `branding.json` (US-23.4).

- Especificação completa: `docs/whitelabel.md` (EN: `docs/en/whitelabel.md`).
- Design system e telas: `docs/ui-ux.md` (EN: `docs/en/ui-ux.md`).

## Desenvolvimento

1. Cores e raios vivem em `src/styles/tokens.css`; presets/destaques em `src/styles/presets.css` (a criar na US-22). Tailwind v4 consome esses tokens.
2. Use classes Tailwind e variantes semânticas; **proibido** hex/rgb/hsl ou valor arbitrário de cor fora de `tokens.css`/`presets.css`, e proibido estilo inline para marca (CSP).
3. Rode `npm run dev` e confira login, shell, vendas, pagamento e produto em 1024×600 e 1280×800.
4. Antes do push: `./scripts/check-all.sh` verde.

## Regras que mantêm o whitelabel

- Nomes de token semânticos (`--surface-*`, nunca `--loja-x`).
- Só tema claro; fontes do sistema (offline-first, sem webfont remota).
- Preset/destaque são enums fechados, validados em AA; nada de tema livre.
- Logo: PNG/WebP ≤ 512 KB, sem SVG; fallback textual.
- JWT nunca em `localStorage`/`sessionStorage`; marca/código da loja em arquivo via IPC.
