//! Cifra em repouso dos segredos do certificado A1 (US09 Task 9.1).
//!
//! AES-256-GCM com nonce aleatório de 12 bytes por operação. O AAD amarra o
//! ciphertext ao tenant e ao campo (`pfx`/`password`): copiar a linha para
//! outra loja ou trocar os campos não decifra.
//!
//! A chave mestra vem de `CERT_MASTER_KEY` (env, 32 bytes em base64) e vive
//! só na memória do processo — nunca é persistida no banco.

use aes_gcm::aead::{Aead, AeadCore, OsRng, Payload};
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroizing;

/// Tamanho da chave mestra (AES-256).
pub const KEY_LEN: usize = 32;
/// Tamanho do nonce GCM (96 bits, recomendação NIST).
pub const NONCE_LEN: usize = 12;

#[derive(Debug, Error)]
pub enum CipherError {
    #[error("chave mestra inválida")]
    InvalidKey,
    #[error("falha ao cifrar segredo do certificado")]
    Encrypt,
    #[error("falha ao decifrar segredo do certificado")]
    Decrypt,
}

/// Cifra autenticada com a chave mestra do processo.
#[derive(Clone)]
pub struct CertificateCipher {
    cipher: Aes256Gcm,
}

impl CertificateCipher {
    pub fn from_key(key: [u8; KEY_LEN]) -> Self {
        Self {
            cipher: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&key)),
        }
    }

    /// Lê a chave mestra em base64 (exatamente 32 bytes decodificados).
    pub fn from_base64(raw: &str) -> Result<Self, CipherError> {
        let bytes = STANDARD
            .decode(raw.trim())
            .map_err(|_| CipherError::InvalidKey)?;
        let key: [u8; KEY_LEN] = bytes.try_into().map_err(|_| CipherError::InvalidKey)?;
        Ok(Self::from_key(key))
    }

    /// Retorna `(nonce, ciphertext)` — ambos opacos; o nonce é público.
    pub fn encrypt(
        &self,
        store_id: Uuid,
        field: &str,
        plaintext: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>), CipherError> {
        let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
        let aad = aad(store_id, field);
        let ciphertext = self
            .cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad: aad.as_bytes(),
                },
            )
            .map_err(|_| CipherError::Encrypt)?;
        Ok((nonce.to_vec(), ciphertext))
    }

    /// Decifra e zera o buffer ao sair de escopo (`Zeroizing`).
    pub fn decrypt(
        &self,
        store_id: Uuid,
        field: &str,
        nonce: &[u8],
        ciphertext: &[u8],
    ) -> Result<Zeroizing<Vec<u8>>, CipherError> {
        if nonce.len() != NONCE_LEN {
            return Err(CipherError::Decrypt);
        }
        let aad = aad(store_id, field);
        self.cipher
            .decrypt(
                Nonce::from_slice(nonce),
                Payload {
                    msg: ciphertext,
                    aad: aad.as_bytes(),
                },
            )
            .map(Zeroizing::new)
            .map_err(|_| CipherError::Decrypt)
    }
}

fn aad(store_id: Uuid, field: &str) -> String {
    format!("store_certificate:v1:{store_id}:{field}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cipher() -> CertificateCipher {
        CertificateCipher::from_key([7u8; KEY_LEN])
    }

    #[test]
    fn roundtrip_devolve_o_segredo() {
        let store = Uuid::new_v4();
        let (nonce, ciphertext) = cipher()
            .encrypt(store, "password", b"senha-super-secreta")
            .unwrap();
        assert_eq!(nonce.len(), NONCE_LEN);
        assert_ne!(ciphertext.as_slice(), b"senha-super-secreta");
        let plain = cipher()
            .decrypt(store, "password", &nonce, &ciphertext)
            .unwrap();
        assert_eq!(plain.as_slice(), b"senha-super-secreta");
    }

    #[test]
    fn chave_errada_nao_decifra() {
        let store = Uuid::new_v4();
        let (nonce, ciphertext) = cipher().encrypt(store, "pfx", b"bytes").unwrap();
        let other = CertificateCipher::from_key([9u8; KEY_LEN]);
        assert!(other.decrypt(store, "pfx", &nonce, &ciphertext).is_err());
    }

    #[test]
    fn nonce_unico_por_operacao() {
        let store = Uuid::new_v4();
        let (n1, c1) = cipher().encrypt(store, "pfx", b"mesmo-conteudo").unwrap();
        let (n2, c2) = cipher().encrypt(store, "pfx", b"mesmo-conteudo").unwrap();
        assert_ne!(n1, n2);
        assert_ne!(c1, c2);
    }

    #[test]
    fn aad_amarra_tenant_e_campo() {
        let (a, b) = (Uuid::new_v4(), Uuid::new_v4());
        let (nonce, ciphertext) = cipher().encrypt(a, "password", b"segredo").unwrap();
        assert!(
            cipher()
                .decrypt(b, "password", &nonce, &ciphertext)
                .is_err(),
            "ciphertext não pode decifrar em outra loja"
        );
        assert!(
            cipher().decrypt(a, "pfx", &nonce, &ciphertext).is_err(),
            "ciphertext não pode decifrar em outro campo"
        );
    }

    #[test]
    fn nonce_com_tamanho_errado_e_rejeitado() {
        let store = Uuid::new_v4();
        assert!(
            cipher()
                .decrypt(store, "pfx", &[0u8; 8], &[0u8; 32])
                .is_err()
        );
    }

    #[test]
    fn chave_base64_exige_32_bytes() {
        let ok = STANDARD.encode([1u8; KEY_LEN]);
        assert!(CertificateCipher::from_base64(&ok).is_ok());
        assert!(CertificateCipher::from_base64("não-é-base64").is_err());
        let short = STANDARD.encode([1u8; 16]);
        assert!(CertificateCipher::from_base64(&short).is_err());
    }
}
