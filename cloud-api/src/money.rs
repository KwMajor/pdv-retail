//! Dinheiro canônico (US03+): UM lugar para parse, validação e normalização.
//!
//! Política (reject-not-round, autoridade backend):
//! - entrada `Decimal` (número JSON) ou `&str` (`"27.99"`, aceita pela serde
//!   do `rust_decimal`);
//! - escala > 2 → `Err` (422 no service). Nunca arredonda silenciosamente
//!   uma 3ª casa: `"10.999"` é rejeitado, não vira `"11.00"`;
//! - escala ≤ 2 → normaliza com `rescale(2)`: `"10"` e `"10.5"` viram
//!   `10.00`/`10.50` (mesmo valor, mesma escala → comparações e `DECIMAL(12,2)`
//!   determinísticos);
//! - serialização é string (`"10.50"`, nunca float) — ver teste abaixo.
//! - ON WRITE apenas: linhas históricas (`SALE_ITEM` congelado) nunca passam
//!   por aqui de novo.

use std::str::FromStr;

use rust_decimal::Decimal;

/// Teto alinhado às colunas `DECIMAL(12,2)`: abaixo de 10^10 (escala ≤ 2).
pub static MONEY_MAX: std::sync::LazyLock<Decimal> =
    std::sync::LazyLock::new(|| Decimal::new(1_000_000_000_000, 2));

/// Entrada string (`&str`): trim → parse → [`canonical_money`].
/// Mensagens em pt-BR (produto); o service mapeia para `Invalid` (422).
pub fn parse_money(raw: &str, field: &str) -> Result<Decimal, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(format!("{field} é obrigatório"));
    }
    let value = Decimal::from_str(trimmed)
        .map_err(|_| format!("{field} deve ser decimal ≥ 0 com até 2 casas"))?;
    canonical_money(value, field)
}

/// Entrada `Decimal` (número JSON já desserializado): valida e normaliza.
/// Rejeita negativo, ≥ `MONEY_MAX` e escala > 2; senão `rescale(2)`.
pub fn canonical_money(value: Decimal, field: &str) -> Result<Decimal, String> {
    if value < Decimal::ZERO || value >= *MONEY_MAX || value.scale() > 2 {
        return Err(format!("{field} deve ser decimal ≥ 0 com até 2 casas"));
    }
    let mut normalized = value;
    normalized.rescale(2);
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aceita_com_2_casas() {
        assert_eq!(
            parse_money("27.99", "price").unwrap(),
            Decimal::new(2799, 2)
        );
    }

    #[test]
    fn normaliza_escala_para_2() {
        // "10" e "10.5" são o mesmo dinheiro que "10.00"/"10.50": valor E
        // escala idênticos após canonicalização (comparação determinística).
        assert_eq!(parse_money("10", "price").unwrap(), Decimal::new(1000, 2));
        assert_eq!(parse_money("10.5", "price").unwrap(), Decimal::new(1050, 2));
        assert_eq!(
            parse_money("  10.50  ", "price").unwrap(),
            Decimal::new(1050, 2)
        );
        assert_eq!(
            canonical_money(Decimal::new(105, 1), "price").unwrap(),
            Decimal::new(1050, 2)
        );
    }

    #[test]
    fn rejeita_terceira_casa_sem_arredondar() {
        // "10.999" NÃO vira "11.00": rejeita em vez de arredondar.
        assert!(parse_money("10.999", "price").is_err());
        assert!(canonical_money(Decimal::new(10999, 3), "price").is_err());
    }

    #[test]
    fn rejeita_lixo_negativo_e_teto() {
        assert!(parse_money("", "price").is_err());
        assert!(parse_money("   ", "price").is_err());
        assert!(parse_money("abc", "price").is_err());
        assert!(parse_money("12,34", "price").is_err());
        assert!(parse_money("-1.00", "price").is_err());
        assert!(parse_money("10000000000.00", "price").is_err());
        assert!(canonical_money(Decimal::new(-1, 0), "price").is_err());
    }

    #[test]
    fn zero_e_limites() {
        assert_eq!(parse_money("0", "price").unwrap(), Decimal::ZERO);
        assert_eq!(parse_money("0.00", "price").unwrap(), Decimal::ZERO);
        // Um centavo abaixo do teto passa; o teto barra.
        assert!(parse_money("9999999999.99", "price").is_ok());
    }

    #[test]
    fn serializa_como_string_nunca_float() {
        // Contrato com o frontend (US06+): dinheiro trafega como string
        // "10.50" — `1.1 + 2.2 != 3.3` em float não nos atinge.
        let v = serde_json::to_value(Decimal::new(1050, 2)).unwrap();
        assert_eq!(v, serde_json::Value::String("10.50".to_string()));
        // E a string volta ao mesmo valor canônico.
        let back: Decimal = serde_json::from_value(v).unwrap();
        assert_eq!(back, Decimal::new(1050, 2));
    }
}
