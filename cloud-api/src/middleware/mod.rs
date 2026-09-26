//! Extractors/middlewares HTTP da Cloud API.
//!
//! - [`tenant::TenantContext`]: identidade JWT (`store_id` + `user_id` + `role`).
//! - [`store_hint::StoreHint`]: dica NÃO autenticada, só para o login.

pub mod store_hint;
pub mod tenant;

pub use store_hint::StoreHint;
pub use tenant::{STORE_ID_HEADER, TenantContext};
