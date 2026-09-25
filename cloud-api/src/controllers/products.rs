//! CRUD de produtos (US03 Task 3.1).
//!
//! - `store_id` sempre do token; NCM obrigatório (8 dígitos) no POST.
//! - `POST/PUT/DELETE` só gestão (403 p/ caixa); `GET` qualquer papel
//!   autenticado (o caixa precisa achar produto no bip — US06).
//! - `DELETE` é soft (`deactivate`); preço sem auditoria até a Task 3.2.

use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::TenantContext;
use crate::models::Product;
use crate::repositories::PgProductRepository;
use crate::services::product_service::{
    CreateProductInput, ProductError, UpdateProductInput, create_product, deactivate_product,
    get_product, list_products, update_product,
};

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[schema(example = json!({"name": "Arroz T1 5kg", "sku": "ARROZ-T1-5KG", "barcode": "7891234567890", "price": "27.99", "cost": "21.50", "ncm": "10063021", "cest": "1234567", "cfop": "5102", "icms_origin": "0", "icms_rate": "18.00"}))]
pub struct CreateProductRequest {
    pub name: String,
    pub sku: String,
    pub barcode: Option<String>,
    pub price: Decimal,
    pub cost: Decimal,
    /// Obrigatório na criação: 8 dígitos numéricos (rigor SEFAZ).
    pub ncm: String,
    pub cest: Option<String>,
    pub cfop: Option<String>,
    pub icms_origin: Option<String>,
    pub icms_rate: Decimal,
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct UpdateProductRequest {
    pub name: Option<String>,
    pub barcode: Option<String>,
    pub price: Option<Decimal>,
    pub cost: Option<Decimal>,
    pub ncm: Option<String>,
    pub cest: Option<String>,
    pub cfop: Option<String>,
    pub icms_origin: Option<String>,
    pub icms_rate: Option<Decimal>,
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
pub struct ListProductsQuery {
    /// Busca em nome/sku/barcode (case-insensitive, tenant isolado).
    pub q: Option<String>,
    pub limit: Option<i64>,
}

/// Espelho público do `Product` (sem segredos envolvidos).
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[schema(example = json!({"id": "11111111-1111-1111-1111-111111111111", "store_id": "22222222-2222-2222-2222-222222222222", "sku": "ARROZ-T1-5KG", "name": "Arroz T1 5kg", "price": "27.99", "is_active": true}))]
pub struct ProductResponse {
    #[schema(value_type = String)]
    pub id: Uuid,
    #[schema(value_type = String)]
    pub store_id: Uuid,
    pub sku: String,
    pub barcode: Option<String>,
    pub name: String,
    #[schema(value_type = String)]
    pub price: Decimal,
    #[schema(value_type = String)]
    pub cost: Decimal,
    pub ncm: Option<String>,
    pub cest: Option<String>,
    pub cfop: Option<String>,
    pub icms_origin: Option<String>,
    #[schema(value_type = String)]
    pub icms_rate: Decimal,
    pub is_active: bool,
}

impl From<Product> for ProductResponse {
    fn from(p: Product) -> Self {
        Self {
            id: p.id,
            store_id: p.store_id,
            sku: p.sku,
            barcode: p.barcode,
            name: p.name,
            price: p.price,
            cost: p.cost,
            ncm: p.ncm,
            cest: p.cest,
            cfop: p.cfop,
            icms_origin: p.icms_origin,
            icms_rate: p.icms_rate,
            is_active: p.is_active,
        }
    }
}

fn pool_of(state: &crate::AppState) -> Result<sqlx::PgPool, AppError> {
    state
        .pool
        .clone()
        .ok_or_else(|| AppError::Internal("banco não configurado".to_string()))
}

fn service_error(e: ProductError) -> AppError {
    match e {
        ProductError::Invalid(m) => AppError::Unprocessable(m),
        ProductError::SkuTaken => {
            AppError::Conflict("sku já cadastrado nesta loja".to_string())
        }
        ProductError::NotFound => AppError::NotFound,
        ProductError::Db(e) => AppError::Db(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/products",
    tag = "produtos",
    security(("bearer" = [])),
    request_body(content = CreateProductRequest),
    responses(
        (status = 201, description = "Produto criado", body = ProductResponse),
        (status = 401, description = "Sem Bearer válido", body = ErrorBody),
        (status = 403, description = "Caixa não cadastra (só gestão)", body = ErrorBody),
        (status = 409, description = "SKU já cadastrado na loja", body = ErrorBody),
        (status = 422, description = "Campo fora do domínio (NCM, preço, etc.)", body = ErrorBody),
    ),
)]
pub async fn create_product_handler(
    ctx: TenantContext,
    State(state): State<crate::AppState>,
    Json(body): Json<CreateProductRequest>,
) -> Result<(StatusCode, Json<ProductResponse>), AppError> {
    ctx.require_manager()?;
    let repo = PgProductRepository::new(pool_of(&state)?);
    let product = create_product(
        &repo,
        ctx.store_id,
        CreateProductInput {
            name: body.name,
            sku: body.sku,
            barcode: body.barcode,
            price: body.price,
            cost: body.cost,
            ncm: body.ncm,
            cest: body.cest,
            cfop: body.cfop,
            icms_origin: body.icms_origin,
            icms_rate: body.icms_rate,
        },
    )
    .await
    .map_err(service_error)?;
    Ok((StatusCode::CREATED, Json(ProductResponse::from(product))))
}

#[utoipa::path(
    get,
    path = "/api/v1/products",
    tag = "produtos",
    security(("bearer" = [])),
    params(ListProductsQuery),
    responses(
        (status = 200, description = "Só ativos da loja do token", body = [ProductResponse]),
        (status = 401, description = "Sem Bearer válido", body = ErrorBody),
    ),
)]
pub async fn list_products_handler(
    ctx: TenantContext,
    State(state): State<crate::AppState>,
    Query(query): Query<ListProductsQuery>,
) -> Result<Json<Vec<ProductResponse>>, AppError> {
    let repo = PgProductRepository::new(pool_of(&state)?);
    let products = list_products(&repo, ctx.store_id, query.q.as_deref(), query.limit.unwrap_or(50))
        .await
        .map_err(service_error)?;
    Ok(Json(products.into_iter().map(ProductResponse::from).collect()))
}

#[utoipa::path(
    get,
    path = "/api/v1/products/{id}",
    tag = "produtos",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "ID do produto")),
    responses(
        (status = 200, description = "Produto da loja (mesmo inativo)", body = ProductResponse),
        (status = 401, description = "Sem Bearer válido", body = ErrorBody),
        (status = 404, description = "Inexistente ou de outra loja", body = ErrorBody),
    ),
)]
pub async fn get_product_handler(
    ctx: TenantContext,
    State(state): State<crate::AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ProductResponse>, AppError> {
    let repo = PgProductRepository::new(pool_of(&state)?);
    let product = get_product(&repo, ctx.store_id, id).await.map_err(service_error)?;
    Ok(Json(ProductResponse::from(product)))
}

#[utoipa::path(
    put,
    path = "/api/v1/products/{id}",
    tag = "produtos",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "ID do produto")),
    request_body(content = UpdateProductRequest),
    responses(
        (status = 200, description = "Produto atualizado", body = ProductResponse),
        (status = 401, description = "Sem Bearer válido", body = ErrorBody),
        (status = 403, description = "Caixa não edita (só gestão)", body = ErrorBody),
        (status = 404, description = "Inexistente ou de outra loja", body = ErrorBody),
        (status = 422, description = "Campo fora do domínio", body = ErrorBody),
    ),
)]
pub async fn update_product_handler(
    ctx: TenantContext,
    State(state): State<crate::AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateProductRequest>,
) -> Result<Json<ProductResponse>, AppError> {
    ctx.require_manager()?;
    let repo = PgProductRepository::new(pool_of(&state)?);
    let product = update_product(
        &repo,
        ctx.store_id,
        id,
        UpdateProductInput {
            name: body.name,
            barcode: body.barcode,
            price: body.price,
            cost: body.cost,
            ncm: body.ncm,
            cest: body.cest,
            cfop: body.cfop,
            icms_origin: body.icms_origin,
            icms_rate: body.icms_rate,
        },
    )
    .await
    .map_err(service_error)?;
    Ok(Json(ProductResponse::from(product)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/products/{id}",
    tag = "produtos",
    security(("bearer" = [])),
    params(("id" = Uuid, Path, description = "ID do produto")),
    responses(
        (status = 200, description = "Soft delete aplicado (is_active=false)", body = ProductResponse),
        (status = 401, description = "Sem Bearer válido", body = ErrorBody),
        (status = 403, description = "Caixa não exclui (só gestão)", body = ErrorBody),
        (status = 404, description = "Inexistente ou de outra loja", body = ErrorBody),
    ),
)]
pub async fn delete_product_handler(
    ctx: TenantContext,
    State(state): State<crate::AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ProductResponse>, AppError> {
    ctx.require_manager()?;
    let repo = PgProductRepository::new(pool_of(&state)?);
    let product = deactivate_product(&repo, ctx.store_id, id)
        .await
        .map_err(service_error)?;
    Ok(Json(ProductResponse::from(product)))
}
