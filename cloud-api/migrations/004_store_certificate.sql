-- US09 Task 9.1: certificado digital A1 (e-CNPJ) por loja.
--
-- Segredos blindados: o .pfx e a senha NUNCA ficam em texto plano no banco.
-- Ambos são cifrados na aplicação com AES-256-GCM (chave mestra em env,
-- `CERT_MASTER_KEY`) e o nonce é único por registro/segredo. O par
-- nonce/ciphertext é opaco para o banco (BYTEA).
--
-- O AAD da cifra amarra o ciphertext ao `store_id` (label + tenant): copiar
-- a linha para outra loja não permite decifrar. Metadados do certificado
-- (subject/serial/validade) são públicos e ficam em claro para a UI.
CREATE TABLE store_certificate (
  store_id            UUID PRIMARY KEY REFERENCES store_settings(id) ON DELETE CASCADE,
  -- CNPJ canônico (só dígitos) extraído/validado no upload.
  cnpj                VARCHAR(14) NOT NULL,
  pfx_nonce           BYTEA NOT NULL,
  pfx_ciphertext      BYTEA NOT NULL,
  password_nonce      BYTEA NOT NULL,
  password_ciphertext BYTEA NOT NULL,
  subject_cn          TEXT NOT NULL,
  serial_hex          TEXT NOT NULL,
  not_before          TIMESTAMPTZ NOT NULL,
  not_after           TIMESTAMPTZ NOT NULL,
  uploaded_by         UUID REFERENCES "user"(id) ON DELETE SET NULL,
  created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  CONSTRAINT store_certificate_cnpj_digits CHECK (cnpj ~ '^[0-9]{14}$'),
  CONSTRAINT store_certificate_nonce_len CHECK (
    octet_length(pfx_nonce) = 12 AND octet_length(password_nonce) = 12
  ),
  -- Limite do DoD (5MB) + tag GCM de 16 bytes.
  CONSTRAINT store_certificate_pfx_size CHECK (
    octet_length(pfx_ciphertext) BETWEEN 17 AND 5242896
  )
);

CREATE TRIGGER trg_store_certificate_updated BEFORE UPDATE ON store_certificate
  FOR EACH ROW EXECUTE FUNCTION set_updated_at();

COMMENT ON TABLE store_certificate IS
  'Certificado A1 da loja. PFX e senha cifrados (AES-256-GCM; AAD=store_id); metadados públicos para a UI.';
COMMENT ON COLUMN store_certificate.password_ciphertext IS
  'Senha do .pfx cifrada. Nunca é devolvida pela API nem gravada em log.';
