//! Regras de negócio isoladas (chamadas pelos controllers).
//!
//! Services nunca montam SQL: delegam aos repositórios, que já exigem
//! `store_id` e usam prepared statements.

pub mod user_service;

pub use user_service::{CreateUserError, CreateUserInput, create_user};
