//! Motor de estoque (US04 Task 4.1): ledger + saldo na mesma transação.
//!
//! O `reason` do banco continua texto (compatível com o histórico), mas a API
//! só aceita o enum fechado [`MovementType`]. Saldo via soma atômica
//! (`add_tx`): sem read-modify-write, sem movimento perdido em concorrência.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{Stock, StockMovement};
use crate::repositories::{
    NewStockMovement, PgProductRepository, PgStockMovementRepository, PgStockRepository,
    ProductRepository, StockMovementRepository, StockRepository,
};

/// Justificativa tipada do movimento (mapeia p/ `reason` no banco).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MovementType {
    Sale,
    ManualAdd,
    Return,
    Loss,
    Adjust,
}

impl MovementType {
    pub fn as_str(self) -> &'static str {
        match self {
            MovementType::Sale => "SALE",
            MovementType::ManualAdd => "MANUAL_ADD",
            MovementType::Return => "RETURN",
            MovementType::Loss => "LOSS",
            MovementType::Adjust => "ADJUST",
        }
    }
}

pub struct ApplyMovementInput {
    pub product_id: Uuid,
    pub qty_delta: Decimal,
    pub movement: MovementType,
    pub ref_sale_id: Option<Uuid>,
}

#[derive(Debug, thiserror::Error)]
pub enum MovementError {
    #[error("{0}")]
    Invalid(String),
    #[error("produto não encontrado")]
    ProductNotFound,
    #[error("erro de banco de dados")]
    Db(#[from] sqlx::Error),
}

fn qty_delta(delta: Decimal) -> Result<Decimal, MovementError> {
    if delta == Decimal::ZERO {
        return Err(MovementError::Invalid(
            "quantidade do movimento não pode ser zero".to_string(),
        ));
    }
    if delta.scale() > 3 {
        return Err(MovementError::Invalid(
            "quantidade aceita no máximo 3 casas decimais".to_string(),
        ));
    }
    if delta.abs() >= Decimal::new(1_000_000_000_000, 3) {
        return Err(MovementError::Invalid(
            "quantidade fora da faixa suportada".to_string(),
        ));
    }
    Ok(delta)
}

/// Registra o movimento no ledger e aplica ao saldo — tudo ou nada.
/// Retorna `(movimento, saldo resultante)`.
pub async fn apply_movement(
    pool: &sqlx::PgPool,
    store_id: Uuid,
    input: ApplyMovementInput,
) -> Result<(StockMovement, Stock), MovementError> {
    let delta = qty_delta(input.qty_delta)?;
    // Produto existe e é da loja? (Ausente ou alheio → NotFound, sem vazar.)
    let products = PgProductRepository::new(pool.clone());
    if products
        .find_by_id(store_id, input.product_id)
        .await?
        .is_none()
    {
        return Err(MovementError::ProductNotFound);
    }

    let mut tx = pool.begin().await.map_err(MovementError::Db)?;
    let movements = PgStockMovementRepository::new(pool.clone());
    let stocks = PgStockRepository::new(pool.clone());
    let movement = movements
        .record_tx(
            &mut tx,
            NewStockMovement {
                store_id,
                product_id: input.product_id,
                qty_delta: delta,
                reason: input.movement.as_str().to_string(),
                ref_sale_id: input.ref_sale_id,
            },
        )
        .await?;
    let stock = stocks
        .add_tx(&mut tx, store_id, input.product_id, delta)
        .await?;
    tx.commit().await.map_err(MovementError::Db)?;
    Ok((movement, stock))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_type_mapeia_reason() {
        assert_eq!(MovementType::Sale.as_str(), "SALE");
        assert_eq!(MovementType::ManualAdd.as_str(), "MANUAL_ADD");
        assert_eq!(MovementType::Return.as_str(), "RETURN");
        assert_eq!(MovementType::Loss.as_str(), "LOSS");
        assert_eq!(MovementType::Adjust.as_str(), "ADJUST");
    }

    #[test]
    fn delta_zero_e_escala_rejeitados() {
        assert!(qty_delta(Decimal::ZERO).is_err());
        assert!(qty_delta(Decimal::new(1, 4)).is_err());
        assert!(qty_delta(Decimal::new(5, 0)).is_ok());
        assert!(qty_delta(Decimal::new(-2, 0)).is_ok());
    }
}
