//! US01 Task 1.2: espelhos Rust das tabelas do `001_initial_schema.sql`.
//!
//! Convenções:
//! - Toda struct deriva `Serialize + Deserialize + FromRow` (DoD).
//! - `uuid::Uuid` para chaves, `rust_decimal::Decimal` para dinheiro/taxas,
//!   `chrono::DateTime<Utc>` para `TIMESTAMPTZ` (o DoD cita `NaiveDateTime`
//!   como exemplo, mas `TIMESTAMPTZ` exige tipo com fuso — `NaiveDateTime`
//!   perderia o offset e é inadequado para carimbo fiscal).
//! - `Option` ⇔ coluna NULLABLE no banco; demais campos são `NOT NULL`.

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

pub use audit_log::AuditLog;
pub use customer::Customer;
pub use fiscal_queue::FiscalQueue;
pub use payment::Payment;
pub use product::Product;
pub use sale::Sale;
pub use sale_item::SaleItem;
pub use stock::Stock;
pub use stock_movement::StockMovement;
pub use store_settings::StoreSettings;
pub use user::User;
