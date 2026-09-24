-- US01: núcleo multi-tenant (Sprint 1).
-- Regras aplicadas:
--   * Toda tabela operacional tem store_id NOT NULL (isolamento lógico por loja).
--   * Dinheiro sempre NUMERIC(12,2), nunca FLOAT.
--   * PRODUCT/USER usam soft-delete (is_active), nunca DELETE físico.
--   * SALE_ITEM congela o fiscal (snapshot) — imune a mudanças futuras no PRODUCT.
--   * STOCK_MOVEMENT e AUDIT_LOG são imutáveis (trigger bloqueia UPDATE/DELETE).

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Triggers utilitários -------------------------------------------------------
CREATE OR REPLACE FUNCTION set_updated_at() RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = NOW();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION forbid_change() RETURNS TRIGGER AS $$
BEGIN
  RAISE EXCEPTION 'tabela % é imutável (somente INSERT)', TG_TABLE_NAME;
END;
$$ LANGUAGE plpgsql;

-- Lojas (tenants) ------------------------------------------------------------
CREATE TABLE store (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name        TEXT NOT NULL,
  cnpj        TEXT NOT NULL UNIQUE,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TRIGGER trg_store_updated BEFORE UPDATE ON store
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Usuários/funcionários (US02). Pin do gerente vai em pin_hash (Argon2). ------
CREATE TABLE "user" (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id      UUID NOT NULL REFERENCES store(id),
  name          TEXT NOT NULL,
  email         TEXT NOT NULL,
  password_hash TEXT NOT NULL,
  role          TEXT NOT NULL CHECK (role IN ('admin', 'manager', 'cashier')),
  pin_hash      TEXT,
  is_active     BOOLEAN NOT NULL DEFAULT TRUE,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (store_id, email)
);
CREATE INDEX idx_user_store ON "user"(store_id);
CREATE TRIGGER trg_user_updated BEFORE UPDATE ON "user"
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Catálogo com parâmetros fiscais (US03). Bloqueio fiscal é na aplicação: ----
-- produto só ativa (is_active=TRUE) com ncm/cfop/origin preenchidos/válidos.
CREATE TABLE product (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id    UUID NOT NULL REFERENCES store(id),
  sku         TEXT NOT NULL,
  barcode     TEXT,
  name        TEXT NOT NULL,
  price       NUMERIC(12,2) NOT NULL CHECK (price >= 0),
  cost        NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (cost >= 0),
  ncm         TEXT,
  cest        TEXT,
  cfop        TEXT,
  icms_origin TEXT,
  icms_rate   NUMERIC(5,2) NOT NULL DEFAULT 0,
  is_active   BOOLEAN NOT NULL DEFAULT TRUE,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (store_id, sku)
);
CREATE INDEX idx_product_store ON product(store_id);
CREATE INDEX idx_product_barcode ON product(store_id, barcode);
CREATE TRIGGER trg_product_updated BEFORE UPDATE ON product
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Clientes PF/PJ unificados (LGPD: minimizar; anonimizar preservando PK). ----
CREATE TABLE customer (
  id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id           UUID NOT NULL REFERENCES store(id),
  name               TEXT NOT NULL,
  cpf_cnpj           TEXT,
  corporate_name     TEXT,
  state_registration TEXT,
  phone              TEXT,
  email              TEXT,
  is_active          BOOLEAN NOT NULL DEFAULT TRUE,
  created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_customer_store ON customer(store_id);
CREATE TRIGGER trg_customer_updated BEFORE UPDATE ON customer
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Vendas. customer_id nulo + anonymous_cpf_cnpj = "CPF na nota" sem cadastro.
CREATE TABLE sale (
  id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id           UUID NOT NULL REFERENCES store(id),
  customer_id        UUID REFERENCES customer(id),
  anonymous_cpf_cnpj TEXT,
  status             TEXT NOT NULL DEFAULT 'closed'
                     CHECK (status IN ('open', 'closed', 'cancelled', 'pending')),
  subtotal           NUMERIC(12,2) NOT NULL DEFAULT 0,
  discount           NUMERIC(12,2) NOT NULL DEFAULT 0,
  total              NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (total >= 0),
  change_amount      NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (change_amount >= 0),
  created_by         UUID REFERENCES "user"(id),
  created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT sale_customer_or_anonymous
    CHECK (customer_id IS NOT NULL OR anonymous_cpf_cnpj IS NULL OR length(anonymous_cpf_cnpj) BETWEEN 11 AND 18)
);
CREATE INDEX idx_sale_store ON sale(store_id);
CREATE INDEX idx_sale_created ON sale(store_id, created_at);

-- Itens com snapshot fiscal (US03/US10): cópia do fiscal vigente na venda. ---
CREATE TABLE sale_item (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  sale_id     UUID NOT NULL REFERENCES sale(id) ON DELETE CASCADE,
  product_id  UUID NOT NULL REFERENCES product(id),
  quantity    NUMERIC(12,3) NOT NULL CHECK (quantity > 0),
  unit_price  NUMERIC(12,2) NOT NULL CHECK (unit_price >= 0),
  discount    NUMERIC(12,2) NOT NULL DEFAULT 0 CHECK (discount >= 0),
  total       NUMERIC(12,2) NOT NULL CHECK (total >= 0),
  ncm_code    TEXT,
  cfop        TEXT,
  icms_rate   NUMERIC(5,2) NOT NULL DEFAULT 0
);
CREATE INDEX idx_sale_item_sale ON sale_item(sale_id);

-- Pagamentos fracionados (US07). Dinheiro: tendered >= amount; troco em sale.
CREATE TABLE payment (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  sale_id         UUID NOT NULL REFERENCES sale(id) ON DELETE CASCADE,
  method          TEXT NOT NULL CHECK (method IN ('cash', 'pix', 'credit', 'debit', 'crediario')),
  amount          NUMERIC(12,2) NOT NULL CHECK (amount > 0),
  tendered_amount NUMERIC(12,2),
  created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT payment_cash_tendered
    CHECK (method <> 'cash' OR (tendered_amount IS NOT NULL AND tendered_amount >= amount))
);
CREATE INDEX idx_payment_sale ON payment(sale_id);

-- Ledger de estoque (US04): somente INSERT, histórico inalterável. ------------
CREATE TABLE stock_movement (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id    UUID NOT NULL REFERENCES store(id),
  product_id  UUID NOT NULL REFERENCES product(id),
  qty_delta   NUMERIC(12,3) NOT NULL CHECK (qty_delta <> 0),
  reason      TEXT NOT NULL,
  ref_sale_id UUID REFERENCES sale(id),
  created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_stock_store_product ON stock_movement(store_id, product_id);
CREATE TRIGGER trg_stock_immutable BEFORE UPDATE OR DELETE ON stock_movement
  FOR EACH ROW EXECUTE FUNCTION forbid_change();

-- Auditoria (US03/US21): preço alterado, override de gerente, etc. -----------
CREATE TABLE audit_log (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id      UUID NOT NULL REFERENCES store(id),
  actor_user_id UUID REFERENCES "user"(id),
  action        TEXT NOT NULL,
  entity        TEXT NOT NULL,
  entity_id     TEXT NOT NULL,
  old_data      JSONB,
  new_data      JSONB,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_audit_store ON audit_log(store_id, created_at);
CREATE TRIGGER trg_audit_immutable BEFORE UPDATE OR DELETE ON audit_log
  FOR EACH ROW EXECUTE FUNCTION forbid_change();

-- Fila fiscal (US13/Sprint 3, criada já para modelar a contingência offline):
-- venda + enqueue na mesma transação (outbox); worker consome com SKIP LOCKED.
CREATE TABLE fiscal_queue (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id      UUID NOT NULL REFERENCES store(id),
  sale_id       UUID NOT NULL REFERENCES sale(id),
  status        TEXT NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending', 'processing', 'done', 'failed')),
  attempts      INTEGER NOT NULL DEFAULT 0,
  next_retry_at TIMESTAMPTZ,
  last_error    TEXT,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_fiscal_pending ON fiscal_queue(status, next_retry_at) WHERE status = 'pending';
CREATE TRIGGER trg_fiscal_updated BEFORE UPDATE ON fiscal_queue
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();
