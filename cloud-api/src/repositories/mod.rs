//! US01 Task 1.2: traits de repositório com isolamento por `store_id`.
//!
//! # Invariante (OWASP A01)
//! Toda função que faz `SELECT`, `UPDATE` ou `DELETE` recebe
//! `store_id: uuid::Uuid` como argumento obrigatório e o usa no `WHERE`
//! (e nas FKs compostas de `sale_item`/`payment`/`stock`). Sem `store_id`
//! não há como montar a query — esquecer o filtro não compila.
//!
//! Exceção: `store_settings` é a tabela raiz (ela É o tenant) e por isso
//! seus métodos não recebem `store_id`.
//!
//! # Segurança (OWASP A03)
//! 100% prepared statements via macros `query!`/`query_as!` com binds
//! (`$1`, `$2`, …). É proibido montar SQL por concatenação neste diretório
//! (há um teste em `tests/` que varre estes arquivos e falha se encontrar
//! montagem dinâmica de SQL).
//!
//! # Mutabilidade por entidade
//! - `user`/`product`/`customer`: sem `DELETE` físico — só `deactivate`
//!   (soft delete preserva FKs do histórico fiscal).
//! - `sale_item`/`payment`/`stock_movement`/`audit_log`: append-only, só
//!   `create` + leituras (snapshot fiscal e ledger não podem ser alterados).
//! - `sale`: `create` + leituras + `set_status` (ex: `cancelled`).
//! - `stock`: leitura + `upsert` (saldo consolidado; o ledger fica em
//!   `stock_movement`).
//! - `store_settings`: único com `delete` (barrado por `ON DELETE RESTRICT`
//!   quando há dados vinculados — comportamento verificado em teste).

pub mod audit_log;
pub mod customer;
pub mod fiscal_queue;
pub mod payment;
pub mod product;
pub mod sale;
pub mod sale_item;
pub mod stock;
pub mod stock_movement;
pub mod store_settings;
pub mod user;

pub use audit_log::{AuditLogRepository, NewAuditLog, PgAuditLogRepository};
pub use customer::{CustomerRepository, NewCustomer, PgCustomerRepository};
pub use fiscal_queue::{FiscalQueueRepository, NewFiscalQueue, PgFiscalQueueRepository};
pub use payment::{NewPayment, PaymentRepository, PgPaymentRepository};
pub use product::{NewProduct, PgProductRepository, ProductRepository};
pub use sale::{NewSale, PgSaleRepository, SaleRepository};
pub use sale_item::{NewSaleItem, PgSaleItemRepository, SaleItemRepository};
pub use stock::{PgStockRepository, StockRepository, UpsertStock};
pub use stock_movement::{NewStockMovement, PgStockMovementRepository, StockMovementRepository};
pub use store_settings::{
    NewStoreSettings, PgStoreSettingsRepository, StoreSettingsRepository,
};
pub use user::{NewUser, PgUserRepository, UserRepository};
