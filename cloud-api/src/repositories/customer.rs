//! Repositório de `customer`. Sem `DELETE` físico: só `deactivate`.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Customer;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCustomer {
    pub store_id: Uuid,
    pub name: String,
    pub cpf_cnpj: Option<String>,
    pub corporate_name: Option<String>,
    pub state_registration: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
}

pub trait CustomerRepository {
    async fn create(&self, input: NewCustomer) -> Result<Customer, sqlx::Error>;
    async fn find_by_id(
        &self,
        store_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Customer>, sqlx::Error>;
    async fn list(
        &self,
        store_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Customer>, sqlx::Error>;
    /// Soft delete (`is_active = FALSE`).
    async fn deactivate(&self, store_id: Uuid, id: Uuid) -> Result<Customer, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct PgCustomerRepository {
    pool: sqlx::PgPool,
}

impl PgCustomerRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl CustomerRepository for PgCustomerRepository {
    async fn create(&self, input: NewCustomer) -> Result<Customer, sqlx::Error> {
        sqlx::query_as!(
            Customer,
            "INSERT INTO customer(store_id, name, cpf_cnpj, corporate_name, state_registration, phone, email)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id, store_id, name, cpf_cnpj, corporate_name, state_registration, phone, email, is_active, created_at, updated_at",
            input.store_id,
            input.name,
            input.cpf_cnpj,
            input.corporate_name,
            input.state_registration,
            input.phone,
            input.email,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn find_by_id(
        &self,
        store_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Customer>, sqlx::Error> {
        sqlx::query_as!(
            Customer,
            "SELECT id, store_id, name, cpf_cnpj, corporate_name, state_registration, phone, email, is_active, created_at, updated_at
             FROM customer WHERE store_id = $1 AND id = $2",
            store_id,
            id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn list(
        &self,
        store_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Customer>, sqlx::Error> {
        sqlx::query_as!(
            Customer,
            "SELECT id, store_id, name, cpf_cnpj, corporate_name, state_registration, phone, email, is_active, created_at, updated_at
             FROM customer WHERE store_id = $1
             ORDER BY name ASC LIMIT $2",
            store_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
    }

    async fn deactivate(&self, store_id: Uuid, id: Uuid) -> Result<Customer, sqlx::Error> {
        sqlx::query_as!(
            Customer,
            "UPDATE customer SET is_active = FALSE WHERE store_id = $1 AND id = $2
             RETURNING id, store_id, name, cpf_cnpj, corporate_name, state_registration, phone, email, is_active, created_at, updated_at",
            store_id,
            id,
        )
        .fetch_one(&self.pool)
        .await
    }
}
