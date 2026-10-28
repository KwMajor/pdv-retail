# Contribuindo

## Fluxo

1. Branch por funcionalidade a partir da `dev` (`us-XX-slug` ou `fix/assunto`).
2. Um commit por entrega, com testes junto (`feat:`, `fix:`, `test:`, `chore:`, `docs:`).
3. `./scripts/check-all.sh` **verde** antes de todo push (o hook pre-push verifica sozinho).
4. Um PR por entrega rumo à `dev`, com a suíte do CI verde.
5. Sem segredos no stage (`.env`, `*.pfx`, `*.pem`, `*.key`) e sem `force-push`.

## Documentação bilíngue

`README.md` e `docs/` têm espelho em inglês (`README_EN.md`, `docs/en/`).
PR que alterar doc PT **exige** atualizar o espelho EN no mesmo branch
(links cruzados nos topos). Descrições do Swagger ficam em inglês.

## Padrões que o CI impõe (e review também)

- **Backend (Rust/Axum/SQLx):** prepared statements sempre (`$1…`, nunca concatenar SQL); `store_id` em toda query operacional; sem `unwrap()`/`panic!` fora de testes (`AppError` no Axum); dinheiro em `rust_decimal::Decimal` (nunca `f32`/`f64`); `cargo fmt` limpo e `clippy` sem warnings.
- **Frontend (React/TS):** TypeScript `strict`, sem `any` e sem `localStorage`/`sessionStorage` (JWT só em memória + cofre via IPC); validadores como funções puras em `lib/` com testes Vitest; Tailwind CSS v4 + design tokens em `src/styles/tokens.css` (fonte da verdade, nenhum hex/rgb/hsl fora dele e sem valor arbitrário de cor); primitivos acessíveis com Radix UI; seguir `docs/ui-ux.md`.
- **Whitelabel:** marca da loja é configuração em runtime (logo/nome/preset/destaque), nunca CSS por loja; presets/destaques são enums fechados e validados em AA; ver `docs/whitelabel.md`.
- **Docs:** mudança que altere como rodar, testar, configurar, a UI ou o contrato da API atualiza `docs/` e `README` no mesmo branch/commit (incluindo UI/UX/whitelabel quando afetados).

## Licença

Ao contribuir você concorda que sua contribuição entra sob a licença **MIT** do projeto (`LICENSE`).
