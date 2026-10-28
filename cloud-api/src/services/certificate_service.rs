//! Upload e armazenamento blindado do certificado A1 (US09 Task 9.1).
//!
//! Fluxo: valida entrada → valida o PKCS#12 de verdade (`openssl::Pkcs12`,
//! confere senha/MAC e extrai X.509) → cifra PFX e senha com AES-256-GCM →
//! upsert transacional + `audit_log` (só metadados públicos) → resposta sem
//! segredos.
//!
//! Regras de ouro:
//! - a senha nunca é logada, ecoada em erro ou gravada em claro;
//! - o CNPJ informado precisa ser válido (14 dígitos + DV), bater com o
//!   cadastro da loja e, quando presente no subject, com o do certificado;
//! - falha em qualquer etapa = rollback (nenhum certificado parcial).

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use chrono::{DateTime, NaiveDateTime, Utc};
use openssl::nid::Nid;
use openssl::pkcs12::Pkcs12;
use openssl::x509::X509Ref;
use uuid::Uuid;
use zeroize::Zeroizing;

use crate::certificate_cipher::{CertificateCipher, CipherError};
use crate::models::StoreCertificate;
use crate::repositories::{
    AuditLogRepository, NewAuditLog, PgAuditLogRepository, PgStoreCertificateRepository,
    PgStoreSettingsRepository, StoreCertificateRepository, StoreSettingsRepository,
    UpsertStoreCertificate,
};

/// Limite do DoD: 5 MB.
pub const MAX_PFX_BYTES: usize = 5 * 1024 * 1024;
/// Senha do certificado: 1 a 128 caracteres.
pub const PASSWORD_MAX_CHARS: usize = 128;

/// Campos do upload (o controller só faz o parse do JSON).
pub struct UploadCertificateInput {
    pub cnpj: String,
    pub password: String,
    pub file_name: String,
    pub file_base64: String,
}

/// Metadados públicos do certificado (nunca inclui PFX nem senha).
pub struct CertificateInfo {
    pub store_id: Uuid,
    pub cnpj: String,
    pub subject_cn: String,
    pub serial_hex: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub uploaded_at: DateTime<Utc>,
}

impl From<StoreCertificate> for CertificateInfo {
    fn from(cert: StoreCertificate) -> Self {
        Self {
            store_id: cert.store_id,
            cnpj: cert.cnpj,
            subject_cn: cert.subject_cn,
            serial_hex: cert.serial_hex,
            not_before: cert.not_before,
            not_after: cert.not_after,
            uploaded_at: cert.updated_at,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CertificateError {
    /// Entrada semanticamente inválida (→ 422), sem detalhe de segredo.
    #[error("{0}")]
    Invalid(String),
    #[error("certificado não encontrado")]
    NotFound,
    #[error("erro de banco de dados")]
    Db(#[from] sqlx::Error),
    #[error("falha na cifra do certificado")]
    Crypto(#[from] CipherError),
}

/// Metadados do certificado (apenas o que pode ir para a UI/auditoria).
pub async fn get_certificate(
    pool: &sqlx::PgPool,
    store_id: Uuid,
) -> Result<CertificateInfo, CertificateError> {
    PgStoreCertificateRepository::new(pool.clone())
        .find_by_store(store_id)
        .await?
        .map(CertificateInfo::from)
        .ok_or(CertificateError::NotFound)
}

/// Valida, cifra e persiste o certificado da loja (upsert atômico + auditoria).
pub async fn upload_certificate(
    pool: &sqlx::PgPool,
    store_id: Uuid,
    uploaded_by: Uuid,
    master_key: [u8; crate::certificate_cipher::KEY_LEN],
    input: UploadCertificateInput,
) -> Result<CertificateInfo, CertificateError> {
    let UploadCertificateInput {
        cnpj,
        password,
        file_name,
        file_base64,
    } = input;
    let password = Zeroizing::new(password);

    validate_extension(&file_name)?;
    validate_password(&password)?;
    let cnpj = normalize_cnpj(&cnpj)?;
    let pfx = decode_pfx(&file_base64)?;
    let parsed = parse_pkcs12(&pfx, &password)?;

    // O CNPJ do payload precisa ser o do cadastro da loja (normalizado).
    let store = PgStoreSettingsRepository::new(pool.clone())
        .find_by_id(store_id)
        .await?
        .ok_or(CertificateError::NotFound)?;
    let store_cnpj = digits_only(&store.cnpj).ok_or_else(|| {
        CertificateError::Invalid(
            "CNPJ da loja no cadastro é inválido; corrija o cadastro antes do upload".to_string(),
        )
    })?;
    if store_cnpj != cnpj {
        return Err(CertificateError::Invalid(
            "CNPJ informado não corresponde ao cadastro da loja".to_string(),
        ));
    }
    // Hardening: quando o subject traz o CNPJ (e-CNPJ), ele também precisa bater.
    if let Some(cert_cnpj) = extract_cnpj_from_cn(&parsed.subject_cn)
        && cert_cnpj != cnpj
    {
        return Err(CertificateError::Invalid(
            "CNPJ do certificado não corresponde ao informado".to_string(),
        ));
    }

    let cipher = CertificateCipher::from_key(master_key);
    let (pfx_nonce, pfx_ciphertext) = cipher.encrypt(store_id, "pfx", &pfx)?;
    let (password_nonce, password_ciphertext) =
        cipher.encrypt(store_id, "password", password.as_bytes())?;

    let certificates = PgStoreCertificateRepository::new(pool.clone());
    let previous = certificates.find_by_store(store_id).await?;

    let mut tx = pool.begin().await?;
    let saved = certificates
        .upsert_tx(
            &mut tx,
            UpsertStoreCertificate {
                store_id,
                cnpj,
                pfx_nonce,
                pfx_ciphertext,
                password_nonce,
                password_ciphertext,
                subject_cn: parsed.subject_cn,
                serial_hex: parsed.serial_hex,
                not_before: parsed.not_before,
                not_after: parsed.not_after,
                uploaded_by: Some(uploaded_by),
            },
        )
        .await?;
    PgAuditLogRepository::new(pool.clone())
        .record_tx(
            &mut tx,
            NewAuditLog {
                store_id,
                actor_user_id: Some(uploaded_by),
                action: "certificate.uploaded".to_string(),
                entity: "store_certificate".to_string(),
                entity_id: store_id.to_string(),
                old_data: previous.as_ref().map(audit_metadata),
                new_data: Some(audit_metadata(&saved)),
            },
        )
        .await?;
    tx.commit().await?;

    Ok(saved.into())
}

/// Auditoria carrega só metadados públicos — jamais senha/PFX.
fn audit_metadata(cert: &StoreCertificate) -> serde_json::Value {
    serde_json::json!({
        "store_id": cert.store_id,
        "cnpj": cert.cnpj,
        "subject_cn": cert.subject_cn,
        "serial_hex": cert.serial_hex,
        "not_before": cert.not_before,
        "not_after": cert.not_after,
        "uploaded_by": cert.uploaded_by,
    })
}

struct ParsedCertificate {
    subject_cn: String,
    serial_hex: String,
    not_before: DateTime<Utc>,
    not_after: DateTime<Utc>,
}

/// Valida o conteúdo binário: extensão não basta — o arquivo precisa ser um
/// PKCS#12 legítimo, com a senha correta e par certificado+chave privada.
fn parse_pkcs12(pfx: &[u8], password: &str) -> Result<ParsedCertificate, CertificateError> {
    let pkcs12 = Pkcs12::from_der(pfx)
        .map_err(|_| CertificateError::Invalid("arquivo não é um PKCS#12 válido".to_string()))?;
    let parsed = pkcs12.parse2(password).map_err(|_| {
        CertificateError::Invalid("senha do certificado inválida ou arquivo corrompido".to_string())
    })?;
    let cert = parsed.cert.ok_or_else(|| {
        CertificateError::Invalid("certificado não contém certificado X.509".to_string())
    })?;
    if parsed.pkey.is_none() {
        return Err(CertificateError::Invalid(
            "certificado não contém chave privada (A1 exige o par)".to_string(),
        ));
    }
    let serial_hex = cert
        .serial_number()
        .to_bn()
        .and_then(|bn| bn.to_hex_str())
        .map(|hex| hex.to_string())
        .map_err(|_| {
            CertificateError::Invalid("não foi possível ler o serial do certificado".to_string())
        })?;
    let not_before = parse_x509_time(&cert.not_before().to_string())?;
    let not_after = parse_x509_time(&cert.not_after().to_string())?;
    Ok(ParsedCertificate {
        subject_cn: subject_common_name(&cert),
        serial_hex,
        not_before,
        not_after,
    })
}

fn subject_common_name(cert: &X509Ref) -> String {
    for entry in cert.subject_name().entries() {
        if entry.object().nid() == Nid::COMMONNAME {
            return match entry.data().to_string() {
                Ok(value) => value,
                Err(_) => String::from_utf8_lossy(entry.data().as_slice()).into_owned(),
            };
        }
    }
    String::new()
}

/// `ASN1_TIME_print` entrega "Oct 28 12:00:00 2026 GMT" (sempre GMT).
fn parse_x509_time(raw: &str) -> Result<DateTime<Utc>, CertificateError> {
    NaiveDateTime::parse_from_str(raw, "%b %e %H:%M:%S %Y GMT")
        .map(|naive| naive.and_utc())
        .map_err(|_| {
            CertificateError::Invalid("não foi possível ler a validade do certificado".to_string())
        })
}

fn validate_extension(file_name: &str) -> Result<(), CertificateError> {
    let name = file_name.trim().to_ascii_lowercase();
    if name.ends_with(".pfx") || name.ends_with(".p12") {
        Ok(())
    } else {
        Err(CertificateError::Invalid(
            "arquivo deve ter extensão .pfx ou .p12".to_string(),
        ))
    }
}

fn validate_password(password: &str) -> Result<(), CertificateError> {
    let chars = password.chars().count();
    if !(1..=PASSWORD_MAX_CHARS).contains(&chars) {
        return Err(CertificateError::Invalid(format!(
            "senha do certificado deve ter de 1 a {PASSWORD_MAX_CHARS} caracteres"
        )));
    }
    Ok(())
}

fn decode_pfx(raw: &str) -> Result<Zeroizing<Vec<u8>>, CertificateError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(CertificateError::Invalid(
            "arquivo do certificado é obrigatório".to_string(),
        ));
    }
    let bytes = STANDARD.decode(trimmed).map_err(|_| {
        CertificateError::Invalid("arquivo deve estar em base64 válido".to_string())
    })?;
    if bytes.is_empty() {
        return Err(CertificateError::Invalid(
            "arquivo do certificado é obrigatório".to_string(),
        ));
    }
    if bytes.len() > MAX_PFX_BYTES {
        return Err(CertificateError::Invalid(
            "arquivo do certificado excede o limite de 5 MB".to_string(),
        ));
    }
    Ok(Zeroizing::new(bytes))
}

/// CNPJ do upload: estritamente 14 dígitos (sem pontuação) + DV.
fn normalize_cnpj(raw: &str) -> Result<String, CertificateError> {
    let value = raw.trim();
    if value.len() != 14 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(CertificateError::Invalid(
            "CNPJ deve ter exatamente 14 dígitos numéricos (sem pontuação)".to_string(),
        ));
    }
    if !is_valid_cnpj(value) {
        return Err(CertificateError::Invalid(
            "CNPJ inválido (dígitos verificadores não conferem)".to_string(),
        ));
    }
    Ok(value.to_string())
}

/// Cadastro legado pode ter CNPJ formatado: normaliza para comparar.
fn digits_only(raw: &str) -> Option<String> {
    let digits: String = raw.chars().filter(char::is_ascii_digit).collect();
    (digits.len() == 14 && is_valid_cnpj(&digits)).then_some(digits)
}

/// Dígito verificador mod-11 do CNPJ (pesos do manual da Receita).
fn cnpj_digit(digits: &[u8], weights: &[u32]) -> u32 {
    let sum: u32 = digits
        .iter()
        .zip(weights)
        .map(|(digit, weight)| u32::from(*digit - b'0') * weight)
        .sum();
    let remainder = sum % 11;
    if remainder < 2 { 0 } else { 11 - remainder }
}

/// 14 dígitos, não todos iguais, com os 2 DVs corretos.
pub fn is_valid_cnpj(digits: &str) -> bool {
    let bytes = digits.as_bytes();
    if bytes.len() != 14 || !bytes.iter().all(u8::is_ascii_digit) {
        return false;
    }
    if bytes.iter().all(|byte| *byte == bytes[0]) {
        return false;
    }
    const W1: [u32; 12] = [5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    const W2: [u32; 13] = [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    let dv1 = cnpj_digit(&bytes[..12], &W1);
    let dv2 = cnpj_digit(&bytes[..13], &W2);
    u32::from(bytes[12] - b'0') == dv1 && u32::from(bytes[13] - b'0') == dv2
}

/// Melhor esforço: e-CNPJ costuma trazer "...:12345678000195" no CN.
fn extract_cnpj_from_cn(cn: &str) -> Option<String> {
    let bytes = cn.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_digit() {
            let start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if index - start == 14 {
                let candidate = &cn[start..index];
                if is_valid_cnpj(candidate) {
                    return Some(candidate.to_string());
                }
            }
        } else {
            index += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cnpj_valida_dv_e_rejeita_invalidos() {
        assert!(is_valid_cnpj("11222333000181"));
        assert!(is_valid_cnpj("12345678000195"));
        assert!(!is_valid_cnpj("11222333000182"));
        assert!(!is_valid_cnpj("11111111111111"));
        assert!(!is_valid_cnpj("123"));
        assert!(!is_valid_cnpj("1122233300018a"));
    }

    #[test]
    fn cnpj_do_upload_e_estrito_sem_pontuacao() {
        assert_eq!(
            normalize_cnpj(" 11222333000181 ").unwrap(),
            "11222333000181"
        );
        assert!(normalize_cnpj("11.222.333/0001-81").is_err());
        assert!(normalize_cnpj("11222333000182").is_err());
        assert!(normalize_cnpj("").is_err());
    }

    #[test]
    fn cnpj_do_cadastro_aceita_formatado() {
        assert_eq!(
            digits_only("11.222.333/0001-81").as_deref(),
            Some("11222333000181")
        );
        assert_eq!(digits_only("cnpj-antigo"), None);
    }

    #[test]
    fn extensao_aceita_pfx_p12_case_insensitive() {
        assert!(validate_extension("certificado.pfx").is_ok());
        assert!(validate_extension("CERTIFICADO.P12").is_ok());
        assert!(validate_extension("certificado.pfx.exe").is_err());
        assert!(validate_extension("certificado.txt").is_err());
        assert!(validate_extension("").is_err());
    }

    #[test]
    fn senha_respeita_1_a_128_caracteres() {
        assert!(validate_password("x").is_ok());
        assert!(validate_password(&"á".repeat(128)).is_ok());
        assert!(validate_password("").is_err());
        assert!(validate_password(&"x".repeat(129)).is_err());
    }

    #[test]
    fn extrai_cnpj_do_cn_quando_presente() {
        assert_eq!(
            extract_cnpj_from_cn("LOJA TESTE LTDA:11222333000181").as_deref(),
            Some("11222333000181")
        );
        assert_eq!(extract_cnpj_from_cn("LOJA TESTE LTDA").as_deref(), None);
        assert_eq!(
            extract_cnpj_from_cn("SERIAL 123456789012345").as_deref(),
            None
        );
    }

    #[test]
    fn base64_invalido_ou_grande_demais_e_rejeitado() {
        assert!(decode_pfx("!!!").is_err());
        assert!(decode_pfx("  ").is_err());
        let big = STANDARD.encode(vec![0u8; MAX_PFX_BYTES + 1]);
        assert!(decode_pfx(&big).is_err());
    }
}
