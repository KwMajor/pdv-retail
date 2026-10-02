# Sistema PDV (Open Source, MIT)

## Sobre o Projeto
Aplicativo multiplataforma open-source para operação de frente de caixa (PDV) e retaguarda gerencial. A solução foi desenhada no modelo Multi-Tenant (SaaS), permitindo que múltiplas lojas independentes utilizem a mesma infraestrutura de banco de dados e backend na nuvem com isolamento total de informações. O frontend roda nativamente no sistema operacional via Tauri, permitindo acesso direto ao hardware do lojista com consumo mínimo de recursos.

## Licença
Distribuído sob a licença **MIT** (ver `LICENSE`): você pode usar, forkar e modificar — inclusive comercialmente — por sua conta e risco, sem garantia de qualquer tipo.

## Aviso Fiscal
Este sistema auxilia na emissão de documentos fiscais (NFC-e/NF-e), mas **a responsabilidade fiscal é integralmente do operador/loja** (parametrização, certificado digital e cumprimento das obrigações junto à SEFAZ e à contabilidade). O autor não responde por autuações, rejeições ou inconsistências decorrentes do uso ou de modificações no código.

## Contexto de Aplicação
A solução foi projetada para atender ao pequeno e médio varejo brasileiro (mercados, lojas de conveniência, autopeças, etc). Nesse cenário, o sistema é dimensionado para:

* Trabalhar com operação rápida no balcão, garantindo navegação por teclado e leitor de código de barras.
* Garantir conformidade fiscal absoluta com a SEFAZ para emissão de NFC-e/NF-e.
* Permitir operações offline ou contingenciais para que a loja nunca pare de vender em caso de queda de internet.
* Suportar arquitetura escalável para crescimento do número de lojas atendidas sem impacto na latência do banco de dados (arquitetura isolada por `store_id`).

---

## Stack Tecnológica
O sistema foi arquitetado visando alta performance, baixo consumo de recursos e licenças permissivas (**MIT, Apache 2.0, BSD, ISC ou PostgreSQL License**), sem amarras de copyleft nas dependências.

**Backend (API na Nuvem)**
*   **Linguagem:** Rust (MIT / Apache 2.0) — Escolhida pela alta concorrência e uso eficiente de memória.
*   **Framework Web:** Axum (MIT) — Framework assíncrono extremamente rápido para APIs REST.
*   **Acesso a Dados:** SQLx (MIT / Apache 2.0) — Construtor de queries assíncrono e seguro contra SQL Injection, dispensando ORMs pesados.
*   **Gerenciamento de Fila (Mensageria):** Filas baseadas no próprio PostgreSQL ou Valkey (BSD) para comunicação assíncrona com a SEFAZ.

**Frontend & Desktop (O PDV)**
*   **Linguagem:** TypeScript (Apache 2.0).
*   **Container Desktop:** Tauri (MIT / Apache 2.0) — Alternativa mais leve e segura ao Electron, utilizando o renderizador nativo do SO em vez de embutir um navegador inteiro.
*   **Framework UI:** React.js (MIT) — Construção dos componentes visuais e regras de tela.
*   **Estilização:** Tailwind CSS (MIT) e componentes headless como Radix UI (MIT) para construção de interface ágil e acessível.
*   **Gerenciamento de Estado:** Zustand (MIT) — Controle do carrinho de compras e turnos do caixa em memória local.

**Banco de Dados**
*   **SGBD:** PostgreSQL (PostgreSQL License) — Banco relacional robusto com excelente suporte para isolamento de dados via `store_id` (Multi-Tenant) e colunas `JSONB` para logs de auditoria.

---

## Como rodar
Pré-requisitos e instalação: `./scripts/setup-dev.sh` (detalhes em `docs/`).
```bash
./scripts/run-all.sh   # postgres + API (:3000) + web (:1420)
```
Documentação de execução e testes: `docs/run-local.md`, `docs/run-tests.md`, `docs/env.md`.

## Funcionalidades
* **Multi-tenancy e segurança:** JWT com papéis (Gerente/Caixa) e isolamento por loja.
* **Catálogo e estoque:** produtos com NCM/CEST/CFOP, auditoria de preço e ledger de estoque imutável.
* **Frente de caixa (em evolução):** carrinho, pagamentos fracionados, troco, orçamentos.
* **Fiscal e tesouraria (planejado):** NFC-e/SEFAZ, impressão térmica, turnos de caixa, fiado, dashboard. Ver Roadmap abaixo.

## Roadmap resumido
* **Pronto:** fundação multi-tenant, autenticação e perfis, cofre de credenciais no desktop, catálogo com fiscal, motor de estoque, documentação OpenAPI/Swagger, CI com supply chain e Semgrep.
* **Planejado:** frente de caixa completa, motor fiscal (NFC-e, contingência offline), tesouraria (turnos, sangria, fiado) e dashboard financeiro.

---

## Créditos

Arquitetado e desenvolvido por [Matheus Ramos](https://github.com/KwMajor) — [LinkedIn](https://www.linkedin.com/in/matheusfcrms/) · [matheuskwta@gmail.com](mailto:matheuskwta@gmail.com).