-- Fix US01 (TASKS.md [ATUALIZAÇÃO DE REGRA]): histórico de custo no item.
--
-- `unit_cost_price` congela o custo de aquisição vigente no momento da venda,
-- para que o Lucro Bruto futuro (US17) use o custo histórico e nunca o atual.
--
-- Regra matemática (anti-NULL): DECIMAL(12,2) NOT NULL DEFAULT 0, como todas
-- as financeiras. `SUM(unit_price - unit_cost_price)` jamais retorna NULL.
-- O DEFAULT 0 é só rede de segurança p/ linhas pré-existentes; o backend em
-- Rust mapeia o campo explicitamente a partir de `product.cost` (mesmo 0.00).

ALTER TABLE sale_item
  ADD COLUMN unit_cost_price DECIMAL(12,2) NOT NULL DEFAULT 0;

COMMENT ON COLUMN sale_item.unit_cost_price IS
  'Snapshot do product.cost no momento da venda (lucro bruto US17). Isento/brinde = 0.00, nunca NULL.';
