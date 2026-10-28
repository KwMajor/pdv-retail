//! OpenAPI/Swagger (US02 Task 2.5).
//!
//! - Schemas derivam do código (`ToSchema` nos DTOs): a doc nunca diverge.
//! - `NUNCA` derivar `ToSchema` em structs com segredo (`User` tem
//!   `password_hash` — só `UserResponse` é documentado).
//! - `routes::DOCUMENTED` é a fonte única cruzada pelo teste anti-drift:
//!   o `app_router` REGISTRA por estas consts e o spec PRECISA contê-las.

use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};

use crate::controllers::{certificate, health, ping, products, sales, session, stock, users};
use crate::errors::ErrorBody;
use crate::models::UserRole;
use crate::services::sale_service::PaymentMethod;

/// Caminhos da API.
pub mod routes {
    pub const HEALTH: &str = "/health";
    pub const READY: &str = "/ready";
    pub const PING: &str = "/api/v1/ping";
    pub const USERS: &str = "/api/v1/users";
    pub const ME: &str = "/api/v1/me";
    pub const LOGIN: &str = "/api/v1/auth/login";
    pub const STOCK_ADJUST: &str = "/api/v1/stock/adjust";
    pub const PRODUCTS: &str = "/api/v1/products";
    pub const PRODUCT_BY_ID: &str = "/api/v1/products/{id}";
    pub const SALES: &str = "/api/v1/sales";
    pub const STORE_CERTIFICATE: &str = "/api/v1/store/certificate";
    pub const OPENAPI_JSON: &str = "/api-docs/openapi.json";
    pub const DOCS_UI: &str = "/docs";

    /// Toda rota de negócio precisa estar no spec (anti-drift).
    pub const DOCUMENTED: &[&str] = &[
        HEALTH,
        READY,
        PING,
        USERS,
        ME,
        LOGIN,
        PRODUCTS,
        PRODUCT_BY_ID,
        STOCK_ADJUST,
        SALES,
        STORE_CERTIFICATE,
    ];
}

/// `Authorization: Bearer <JWT>` como esquema nomeado `bearer`.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        // Mescla no components JÁ gerado (schemas): nunca substituir por
        // completo, senão os schemas somem do spec.
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        health::health,
        health::ready,
        ping::ping,
        users::create_user_handler,
        users::me_handler,
        session::login_handler,
        products::create_product_handler,
        products::list_products_handler,
        products::get_product_handler,
        products::update_product_handler,
        products::delete_product_handler,
        stock::adjust_stock_handler,
        sales::create_sale_handler,
        certificate::upload_certificate_handler,
        certificate::get_certificate_handler,
    ),
    components(
        schemas(
            health::StatusResponse,
            health::DegradedResponse,
            ping::PingResponse,
            users::CreateUserRequest,
            users::UserResponse,
            session::LoginRequest,
            session::LoginResponse,
            products::CreateProductRequest,
            products::UpdateProductRequest,
            products::ListProductsQuery,
            products::ProductResponse,
            stock::AdjustStockRequest,
            stock::AdjustStockItem,
            stock::StockResponse,
            sales::CreateSaleRequest,
            sales::CreateSaleItemRequest,
            sales::CreateSalePaymentRequest,
            sales::SaleResponse,
            sales::SaleItemResponse,
            sales::SalePaymentResponse,
            certificate::UploadCertificateRequest,
            certificate::CertificateResponse,
            PaymentMethod,
            UserRole,
            ErrorBody,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "sistema", description = "Liveness, readiness and identity"),
        (name = "isolamento", description = "Authenticated tenant proof"),
        (name = "usuarios", description = "Employee management (managers)"),
        (name = "auth", description = "JWT issuance"),
        (name = "produtos", description = "Catalog and fiscal parameters"),
        (name = "estoque", description = "Ledger and adjustments (managers)"),
        (name = "vendas", description = "Sales closing with split payments"),
        (name = "fiscal", description = "Digital certificate A1 (admin only)"),
    ),
)]
pub struct ApiDoc;
