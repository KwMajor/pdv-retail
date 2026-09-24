-- US01 Task 1.1: Arquitetura Multi-Tenant e Banco de Dados (Fundação SaaS).
--
-- Regras aplicadas (DoD):
--   * Arquivo 001_initial_schema.sql com todas as tabelas principais:
--     STORE_SETTINGS, USER, PRODUCT, CUSTOMER, SALE, SALE_ITEM, STOCK, PAYMENT.
--   * Toda tabela operacional possui store_id UUID NOT NULL como FOREIGN KEY
--     referenciando STORE_SETTINGS(id) com ON DELETE RESTRICT explícito.
--   * PRODUCT e USER possuem is_active BOOLEAN NOT NULL DEFAULT TRUE (soft delete).
--   * SALE_ITEM congela o fiscal (Tax Snapshot) com tipagem rigorosa
--     VARCHAR (códigos) e DECIMAL (valores/alíquotas).
--   * Dinheiro sempre DECIMAL(12,2), quantidade DECIMAL(12,3), nunca FLOAT.
--   * SALE_ITEM e PAYMENT carregam store_id + FK composta (sale_id, store_id)
--     para tornar impossível misturar tenants no banco (OWASP A01).
--   * STOCK_MOVEMENT e AUDIT_LOG são imutáveis (somente INSERT).
--
-- Extras já modelados para as próximas USs (não violam o DoD):
--   STOCK_MOVEMENT (ledger US04), AUDIT_LOG (US03/US21), FISCAL_QUEUE (outbox US13).

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

-- Lojas (tenants). Tabela raiz: nunca recebe store_id (ela É o tenant). ------
CREATE TABLE store_settings (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  name        VARCHAR(255) NOT NULL,
  -- VARCHAR(64): comporta CNPJ formatado (18) + margem para tags de teste/seed.
  cnpj        VARCHAR(64) NOT NULL UNIQUE,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE TRIGGER trg_store_settings_updated BEFORE UPDATE ON store_settings
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Usuários/funcionários (US02). Pin do gerente vai em pin_hash (Argon2). ------
-- Soft delete via is_active: nunca DELETE físico (preserva FKs do histórico).
CREATE TABLE "user" (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id      UUID NOT NULL REFERENCES store_settings(id) ON DELETE RESTRICT,
  name          VARCHAR(255) NOT NULL,
  email         VARCHAR(255) NOT NULL,
  password_hash TEXT NOT NULL,
  role          VARCHAR(16) NOT NULL CHECK (role IN ('admin', 'manager', 'cashier')),
  pin_hash      TEXT,
  is_active     BOOLEAN NOT NULL DEFAULT TRUE,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (store_id, email)
);
CREATE INDEX idx_user_store ON "user"(store_id);
CREATE TRIGGER trg_user_updated BEFORE UPDATE ON "user"
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Catálogo com parâmetros fiscais (US03). ------------------------------------
-- Bloqueio fiscal é na aplicação: produto só ativa (is_active=TRUE)
-- com ncm/cfop/origin preenchidos/válidos.
CREATE TABLE product (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id    UUID NOT NULL REFERENCES store_settings(id) ON DELETE RESTRICT,
  sku         VARCHAR(64) NOT NULL,
  barcode     VARCHAR(64),
  name        VARCHAR(255) NOT NULL,
  price       DECIMAL(12,2) NOT NULL CHECK (price >= 0),
  cost        DECIMAL(12,2) NOT NULL DEFAULT 0 CHECK (cost >= 0),
  ncm         VARCHAR(8),
  cest        VARCHAR(7),
  cfop        VARCHAR(4),
  icms_origin VARCHAR(1),
  icms_rate   DECIMAL(5,2) NOT NULL DEFAULT 0,
  is_active   BOOLEAN NOT NULL DEFAULT TRUE,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (store_id, sku),
  -- Âncora para FKs compostas (garantem mesmo tenant em tabelas filhas).
  UNIQUE (id, store_id)
);
CREATE INDEX idx_product_store ON product(store_id);
CREATE INDEX idx_product_barcode ON product(store_id, barcode);
CREATE TRIGGER trg_product_updated BEFORE UPDATE ON product
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Clientes PF/PJ unificados (LGPD: minimizar; anonimizar preservando PK). ----
CREATE TABLE customer (
  id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id           UUID NOT NULL REFERENCES store_settings(id) ON DELETE RESTRICT,
  name               VARCHAR(255) NOT NULL,
  cpf_cnpj           VARCHAR(18),
  corporate_name     VARCHAR(255),
  state_registration VARCHAR(32),
  phone              VARCHAR(32),
  email              VARCHAR(255),
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
  store_id           UUID NOT NULL REFERENCES store_settings(id) ON DELETE RESTRICT,
  customer_id        UUID REFERENCES customer(id) ON DELETE RESTRICT,
  anonymous_cpf_cnpj VARCHAR(18),
  status             VARCHAR(16) NOT NULL DEFAULT 'closed'
                     CHECK (status IN ('open', 'closed', 'cancelled', 'pending')),
  subtotal           DECIMAL(12,2) NOT NULL DEFAULT 0,
  discount           DECIMAL(12,2) NOT NULL DEFAULT 0,
  total              DECIMAL(12,2) NOT NULL DEFAULT 0 CHECK (total >= 0),
  change_amount      DECIMAL(12,2) NOT NULL DEFAULT 0 CHECK (change_amount >= 0),
  created_by         UUID REFERENCES "user"(id) ON DELETE RESTRICT,
  created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT sale_customer_or_anonymous
    CHECK (customer_id IS NOT NULL OR anonymous_cpf_cnpj IS NULL OR length(anonymous_cpf_cnpj) BETWEEN 11 AND 18),
  -- Âncora para FKs compostas das filhas (sale_item, payment, fiscal_queue).
  UNIQUE (id, store_id)
);
CREATE INDEX idx_sale_store ON sale(store_id);
CREATE INDEX idx_sale_created ON sale(store_id, created_at);

-- Itens com snapshot fiscal (Tax Snapshot): cópia do fiscal vigente na venda.
-- store_id próprio + FK composta (sale_id, store_id) impedem item cross-tenant.
-- product vinculado via FK composta (product_id, store_id): produto sempre da
-- mesma loja da venda. Imune a mudanças futuras no PRODUCT.
CREATE TABLE sale_item (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id    UUID NOT NULL REFERENCES store_settings(id) ON DELETE RESTRICT,
  sale_id     UUID NOT NULL,
  product_id  UUID NOT NULL,
  quantity    DECIMAL(12,3) NOT NULL CHECK (quantity > 0),
  unit_price  DECIMAL(12,2) NOT NULL CHECK (unit_price >= 0),
  discount    DECIMAL(12,2) NOT NULL DEFAULT 0 CHECK (discount >= 0),
  total       DECIMAL(12,2) NOT NULL CHECK (total >= 0),
  ncm_code    VARCHAR(8),
  cest        VARCHAR(7),
  cfop        VARCHAR(4),
  icms_origin VARCHAR(1),
  icms_rate   DECIMAL(5,2) NOT NULL DEFAULT 0,
  CONSTRAINT fk_sale_item_sale
    FOREIGN KEY (sale_id, store_id) REFERENCES sale(id, store_id) ON DELETE CASCADE,
  CONSTRAINT fk_sale_item_product
    FOREIGN KEY (product_id, store_id) REFERENCES product(id, store_id) ON DELETE RESTRICT
);
CREATE INDEX idx_sale_item_sale ON sale_item(sale_id);
CREATE INDEX idx_sale_item_store ON sale_item(store_id);

-- Estoque consolidado (saldo atual por produto/loja). ------------------------
-- Ledger imutável fica em stock_movement (US04); aqui é a leitura rápida.
CREATE TABLE stock (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id    UUID NOT NULL REFERENCES store_settings(id) ON DELETE RESTRICT,
  product_id  UUID NOT NULL,
  quantity    DECIMAL(12,3) NOT NULL DEFAULT 0,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  UNIQUE (store_id, product_id),
  CONSTRAINT fk_stock_product
    FOREIGN KEY (product_id, store_id) REFERENCES product(id, store_id) ON DELETE RESTRICT
);
CREATE INDEX idx_stock_store_product ON stock(store_id, product_id);
CREATE TRIGGER trg_stock_updated BEFORE UPDATE ON stock
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();

-- Pagamentos fracionados (US07). Dinheiro: tendered >= amount; troco em sale.
-- store_id próprio + FK composta (sale_id, store_id): pagamento cross-tenant
-- é rejeitado pelo banco, não só pela aplicação.
CREATE TABLE payment (
  id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id        UUID NOT NULL REFERENCES store_settings(id) ON DELETE RESTRICT,
  sale_id         UUID NOT NULL,
  method          VARCHAR(16) NOT NULL
                  CHECK (method IN ('cash', 'pix', 'credit', 'debit', 'crediario')),
  amount          DECIMAL(12,2) NOT NULL CHECK (amount > 0),
  tendered_amount DECIMAL(12,2),
  created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT fk_payment_sale
    FOREIGN KEY (sale_id, store_id) REFERENCES sale(id, store_id) ON DELETE CASCADE,
  CONSTRAINT payment_cash_tendered
    CHECK (method <> 'cash' OR (tendered_amount IS NOT NULL AND tendered_amount >= amount))
);
CREATE INDEX idx_payment_sale ON payment(sale_id);
CREATE INDEX idx_payment_store ON payment(store_id);

-- Ledger de estoque (US04): somente INSERT, histórico inalterável. ------------
CREATE TABLE stock_movement (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id    UUID NOT NULL REFERENCES store_settings(id) ON DELETE RESTRICT,
  product_id  UUID NOT NULL,
  qty_delta   DECIMAL(12,3) NOT NULL CHECK (qty_delta <> 0),
  reason      VARCHAR(64) NOT NULL,
  ref_sale_id UUID REFERENCES sale(id) ON DELETE RESTRICT,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT fk_stock_movement_product
    FOREIGN KEY (product_id, store_id) REFERENCES product(id, store_id) ON DELETE RESTRICT
);
CREATE INDEX idx_stock_movement_store_product ON stock_movement(store_id, product_id);
CREATE TRIGGER trg_stock_movement_immutable BEFORE UPDATE OR DELETE ON stock_movement
  FOR EACH ROW EXECUTE FUNCTION forbid_change();

-- Auditoria (US03/US21): preço alterado, override de gerente, etc. -----------
CREATE TABLE audit_log (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id      UUID NOT NULL REFERENCES store_settings(id) ON DELETE RESTRICT,
  actor_user_id UUID REFERENCES "user"(id) ON DELETE RESTRICT,
  action        VARCHAR(64) NOT NULL,
  entity        VARCHAR(64) NOT NULL,
  entity_id     VARCHAR(64) NOT NULL,
  old_data      JSONB,
  new_data      JSONB,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX idx_audit_store ON audit_log(store_id, created_at);
CREATE TRIGGER trg_audit_immutable BEFORE UPDATE OR DELETE ON audit_log
  FOR EACH ROW EXECUTE FUNCTION forbid_change();

-- Fila fiscal (US13, criada já para modelar a contingência offline): ---------
-- venda + enqueue na mesma transação (outbox); worker consome com SKIP LOCKED.
CREATE TABLE fiscal_queue (
  id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  store_id      UUID NOT NULL REFERENCES store_settings(id) ON DELETE RESTRICT,
  sale_id       UUID NOT NULL,
  status        VARCHAR(16) NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending', 'processing', 'done', 'failed')),
  attempts      INTEGER NOT NULL DEFAULT 0,
  next_retry_at TIMESTAMPTZ,
  last_error    TEXT,
  created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT fk_fiscal_sale
    FOREIGN KEY (sale_id, store_id) REFERENCES sale(id, store_id) ON DELETE RESTRICT
);
CREATE INDEX idx_fiscal_pending ON fiscal_queue(status, next_retry_at) WHERE status = 'pending';
CREATE TRIGGER trg_fiscal_updated BEFORE UPDATE ON fiscal_queue
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();
