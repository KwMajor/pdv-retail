# Testes

*English version: [en/run-tests.md](en/run-tests.md).*

## Portão único (local = CI)

```bash
./scripts/check-all.sh
```

Espelha os jobs do CI e **precisa estar verde antes de todo `git push`**
(o hook `.githooks/pre-push` executa sozinho após
`git config core.hooksPath .githooks`, uma vez por clone).

Ordem: `cargo fmt --check` → `clippy -D warnings` → postgres + `migrate` +
`cargo test` → `sqlx prepare --check` → `cargo-deny` → `semgrep` →
`tsc` + `eslint` + `vitest` + `vite build` + `npm audit`.

## Os dois bancos (não misturar)

| Banco | URL default | Uso |
|---|---|---|
| `pdv` | `DATABASE_URL` | desenvolvimento e **teste manual** (seed, DBeaver) |
| `pdv_test` | `TEST_DATABASE_URL` | **suíte automatizada** (criado e migrado pelo próprio harness) |

## Por camada

```bash
# Backend (em cloud-api/): unit + integração (exige postgres no ar)
DATABASE_URL=postgres://pdv:pdv@localhost:5432/pdv cargo test

# Tipos consultados no banco em compilação: após mudar SQL, rode
cargo sqlx prepare   # (CI confere com --check)

# Frontend (em desktop-client/): tipos, lint, testes e build
./node_modules/.bin/tsc --noEmit
./node_modules/.bin/eslint src/
npm test             # vitest (validadores, componentes, gating)
npm run build

# Segurança supply chain
cargo deny --manifest-path cloud-api/Cargo.toml check
```

## Convenções que os testes impõem

- Dinheiro serializa como **string** (`"27.99"`) e compara **numericamente**
  (escala `15` vs `15.00` não é canônica).
- `store_id` em toda query operacional; cross-tenant responde 404/None.
- Repositórios: só prepared statements (`$1…`), sem `format!` (há teste que
  varre o diretório). Frontend: sem `localStorage` (há teste que varre `src/`).
