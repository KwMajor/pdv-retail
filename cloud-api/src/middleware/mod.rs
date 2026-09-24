//! Extractors/middlewares HTTP da Cloud API.
//!
//! Nesta US mora o [`tenant::TenantContext`]. Na US02 a fonte do `store_id`
//! migra do header temporário `X-Store-ID` para o payload do JWT — a
//! assinatura do extractor (e portanto todos os handlers) permanece igual.

pub mod tenant;

pub use tenant::{StoreHint, TenantContext, STORE_ID_HEADER};
