# PDV System (Open Source, MIT)

*Leia em português: [README.md](README.md).*

## About the Project
Open-source multiplatform app for point-of-sale (POS) operation and back-office management. The solution is designed as Multi-Tenant (SaaS), allowing multiple independent stores to share the same database and cloud backend infrastructure with full data isolation. The frontend runs natively on the operating system via Tauri, enabling direct access to the shop's hardware with minimal resource usage.

## License
Distributed under the **MIT** license (see `LICENSE`): you may use, fork and modify — including commercially — at your own risk, with no warranty of any kind.

## Fiscal Notice
This system assists in issuing fiscal documents (NFC-e/NF-e), but **fiscal liability lies entirely with the operator/store** (parameter setup, digital certificate, and compliance with SEFAZ and accounting obligations). The author is not liable for fines, rejections, or inconsistencies arising from use or code modifications.

## Application Context
The solution targets small and medium Brazilian retail (markets, convenience stores, auto parts, etc). In this scenario, the system is sized to:

* Work fast at the counter, with keyboard navigation and barcode reader support.
* Ensure full fiscal compliance with SEFAZ for NFC-e/NF-e issuance.
* Operate offline or in contingency so the store never stops selling if the internet drops.
* Scale the number of stores served with no database latency impact (`store_id`-isolated architecture).

---

## Tech Stack
The system is architected for high performance, low resource usage, and permissive licenses (**MIT, Apache 2.0, BSD, ISC or PostgreSQL License**), with no copyleft strings attached.

**Backend (Cloud API)**
*   **Language:** Rust (MIT / Apache 2.0) — Chosen for high concurrency and efficient memory use.
*   **Web Framework:** Axum (MIT) — Extremely fast async framework for REST APIs.
*   **Data Access:** SQLx (MIT / Apache 2.0) — Async, SQL-injection-safe query builder, no heavy ORMs.
*   **Queue Management (Messaging):** Queues backed by PostgreSQL itself or Valkey (BSD) for async SEFAZ communication.

**Frontend & Desktop (The POS)**
*   **Language:** TypeScript (Apache 2.0).
*   **Desktop Container:** Tauri (MIT / Apache 2.0) — Lighter, safer alternative to Electron, using the OS native renderer instead of embedding a whole browser.
*   **UI Framework:** React.js (MIT) — Visual components and screen rules.
*   **Styling:** Design tokens in plain CSS + headless Radix UI components (MIT) for agile, accessible UI.
*   **State Management:** Zustand (MIT) — Shopping cart and cashier shift control in local memory.

**Database**
*   **DBMS:** PostgreSQL (PostgreSQL License) — Robust relational database with excellent data-isolation support via `store_id` (Multi-Tenant) and `JSONB` columns for audit logs.

---

## How to Run
Prerequisites and install: `./scripts/setup-dev.sh` (details in `docs/en/`).
```bash
./scripts/run-all.sh   # postgres + API (:3000) + web (:1420)
```
Run and test docs: `docs/en/run-local.md`, `docs/en/run-tests.md`, `docs/en/env.md`.

## Features
* **Multi-tenancy and security:** JWT with roles (Manager/Cashier) and per-store isolation.
* **Catalog and stock:** products with NCM/CEST/CFOP, price audit and immutable stock ledger.
* **Checkout (evolving):** cart, split payments, change, quotes.
* **Fiscal and treasury (planned):** NFC-e/SEFAZ, thermal printing, cashier shifts, store credit, dashboard. See Roadmap below.

## Abridged Roadmap
* **Done:** multi-tenant foundation, auth and profiles, desktop credential vault, fiscal catalog, stock engine, OpenAPI/Swagger docs, CI with supply chain and Semgrep.
* **Planned:** full checkout, fiscal engine (NFC-e, offline contingency), treasury (shifts, cash drops, store credit) and financial dashboard.

---

## Credits

Architected and developed by [Matheus Ramos](https://github.com/KwMajor) — [LinkedIn](https://www.linkedin.com/in/matheusfcrms/) · [matheuskwta@gmail.com](mailto:matheuskwta@gmail.com).
