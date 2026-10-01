# Re-skin em 5 minutos (whitelabel)

1. Edite **só** `tokens.css`: troque `--primary`, `--surface-*`, `--text-*`,
   `--radius-*` e `--font-sans`. Nada fora daqui tem cor, fonte ou raio.
2. Rode `npm run dev` e confira login, navegação e cadastro de produto.
3. Regras que mantêm o whitelabel:
   - Proibido hex/rgb/hsl fora de `tokens.css` (conferir com
     `grep -rnE "#[0-9a-fA-F]{3,8}|rgb\(|hsl\(" src --include="*.tsx" --include="*.ts" --include="*.css" | grep -v styles/tokens.css` deve sair vazio).
   - Nomes de token semânticos (`--surface-*`, nunca `--loja-x`).
   - Só tema claro (sem dark mode por decisão).
   - Fontes do sistema (offline-first: sem webfont remota).
