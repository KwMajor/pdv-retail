# Tests

*Versão em português: [../run-tests.md](../run-tests.md).*

## Single gate (local = CI)

```bash
./scripts/check-all.sh
```

Mirrors the CI jobs and **must be green before every `git push`**
(the `.githooks/pre-push` hook runs it automatically after
`git config core.hooksPath .githooks`, once per clone).

Order: `cargo fmt --check` → `clippy -D warnings` → postgres + `migrate` +
`cargo test` → `sqlx prepare --check` → `cargo-deny` → `semgrep` →
`tsc` + `eslint` + `vitest` + `vite build` + `npm audit`.

## The two databases (don't mix)

| Database | Default URL | Use |
|---|---|---|
| `pdv` | `DATABASE_URL` | development and **manual testing** (seed, DBeaver) |
| `pdv_test` | `TEST_DATABASE_URL` | **automated suite** (created and migrated by its own harness) |

## Per layer

```bash
# Backend (in cloud-api/): unit + integration (needs postgres up)
DATABASE_URL=postgres://pdv:pdv@localhost:5432/pdv cargo test

# DB-checked types at compile time: after changing SQL, run
cargo sqlx prepare   # (CI verifies with --check)

# Frontend (in desktop-client/): types, lint, tests and build
./node_modules/.bin/tsc --noEmit
./node_modules/.bin/eslint src/
npm test             # vitest (validators, components, gating)
npm run build

# Supply-chain security
cargo deny --manifest-path cloud-api/Cargo.toml check
```

## Conventions enforced by tests

- Money serializes as **string** (`"27.99"`) and compares **numerically**
  (scale `15` vs `15.00` is not canonical).
- `store_id` in every operational query; cross-tenant answers 404/None.
- Repositories: prepared statements only (`$1…`), no `format!` (a test scans
  the directory). Frontend: no `localStorage` (a test scans `src/`).
