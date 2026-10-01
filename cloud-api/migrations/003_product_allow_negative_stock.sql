-- US04 Task 4.2: flag de saldo negativo por produto.
--
-- O edge case da task exige "produtos configurados para permitir saldo
-- negativo" (ex: serviço sob encomenda). Default FALSE: trava conservadora.
-- Checado na mesma transação do ajuste (sem TOCTOU).

ALTER TABLE product
  ADD COLUMN allow_negative_stock BOOLEAN NOT NULL DEFAULT FALSE;

COMMENT ON COLUMN product.allow_negative_stock IS
  'TRUE libera saldo negativo (encomenda/serviço). Default FALSE: ajuste que zera abaixo bloqueia com 422.';
