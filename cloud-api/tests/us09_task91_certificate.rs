//! US09 Task 9.1 — upload e armazenamento blindado do certificado A1.
//!
//! - Happy: admin envia PKCS#12 válido; PFX e senha ficam cifrados no banco
//!   (decifráveis só com a chave mestra), metadados públicos na resposta e
//!   auditoria gravada sem segredos.
//! - Segurança: senha nunca volta em GET/POST; chave mestra amarra ao tenant;
//!   RBAC admin-only (manager/cashier 403, sem token 401).
//! - Validação: extensão, base64, 5 MB, senha 1–128, CNPJ 14 dígitos + DV,
//!   match com o cadastro e com o CNPJ do subject, senha errada/corrompido.
//! - OpenAPI: rota e schemas no spec (anti-drift coberto também pela US02).

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use openssl::asn1::Asn1Time;
use openssl::bn::{BigNum, MsbOption};
use openssl::hash::MessageDigest;
use openssl::nid::Nid;
use openssl::pkcs12::Pkcs12;
use openssl::pkey::PKey;
use openssl::rsa::Rsa;
use openssl::x509::{X509, X509NameBuilder};
use pdv_cloud_api::{
    AppState, app_router,
    certificate_cipher::CertificateCipher,
    jwt::JwtKeys,
    models::UserRole,
    openapi::routes,
    repositories::PgUserRepository,
    services::{CreateUserInput, create_user},
};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::OnceCell;
use tower::ServiceExt;
use uuid::Uuid;

static SEQ: AtomicU64 = AtomicU64::new(1_900_001);

/// Segredo só dos testes (nunca em prod).
const TEST_SECRET: &str = "test-only-secret-com-mais-de-32-chars";
/// Chave mestra só dos testes (AppState é montado direto, sem env).
const TEST_CERT_KEY: [u8; 32] = [7u8; 32];

fn uniq(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    static START_MS: std::sync::OnceLock<u128> = std::sync::OnceLock::new();
    let start = *START_MS.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    });
    format!(
        "{prefix}-p{}-t{}-{}",
        std::process::id(),
        start,
        SEQ.fetch_add(1, Ordering::SeqCst)
    )
}

fn test_db_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pdv:pdv@localhost:5432/pdv_test".to_string())
}

async fn pool() -> sqlx::PgPool {
    static DONE: OnceCell<()> = OnceCell::const_new();
    DONE.get_or_init(|| async {
        let url = test_db_url();
        let (base, db) = url.rsplit_once('/').expect("TEST_DATABASE_URL sem /banco");
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&format!("{base}/postgres"))
            .await
            .expect("postgres de teste inacessível — suba com: docker compose up -d");
        let _ = sqlx::query(&format!("CREATE DATABASE \"{db}\""))
            .execute(&admin)
            .await;
        let setup = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(10))
            .connect_lazy(&url)
            .expect("TEST_DATABASE_URL inválida");
        sqlx::migrate!("./migrations")
            .run(&setup)
            .await
            .expect("falha ao aplicar migrations");
    })
    .await;
    PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(10))
        .connect_lazy(&test_db_url())
        .expect("TEST_DATABASE_URL inválida")
}

/// CNPJ válido (DV mod-11) e único por chamada — `store_settings.cnpj` é UNIQUE.
fn next_cnpj() -> String {
    // Inclui o instante de início do processo: reexecutar a suíte não colide
    // com lojas deixadas pela execução anterior no banco de teste.
    use std::time::{SystemTime, UNIX_EPOCH};
    static START_MS: std::sync::OnceLock<u128> = std::sync::OnceLock::new();
    let start = *START_MS.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    });
    let seq = u128::from(SEQ.fetch_add(1, Ordering::SeqCst));
    let base = format!("{:012}", (start * 1_000 + seq) % 1_000_000_000_000);
    let digit = |digits: &str, weights: &[u32]| -> u32 {
        let sum: u32 = digits
            .bytes()
            .zip(weights)
            .map(|(byte, weight)| u32::from(byte - b'0') * weight)
            .sum();
        let remainder = sum % 11;
        if remainder < 2 { 0 } else { 11 - remainder }
    };
    const W1: [u32; 12] = [5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    const W2: [u32; 13] = [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    let dv1 = digit(&base, &W1);
    let with_dv1 = format!("{base}{dv1}");
    let dv2 = digit(&with_dv1, &W2);
    format!("{with_dv1}{dv2}")
}

async fn mk_store(p: &sqlx::PgPool, tag: &str) -> (String, String) {
    let cnpj = next_cnpj();
    let id = sqlx::query_scalar(
        "INSERT INTO store_settings(name, cnpj) VALUES ($1, $2) RETURNING id::text",
    )
    .bind(format!("Loja {tag}"))
    .bind(&cnpj)
    .fetch_one(p)
    .await
    .unwrap();
    (id, cnpj)
}

async fn mk_store_with_cnpj(p: &sqlx::PgPool, tag: &str, cnpj: &str) -> String {
    sqlx::query_scalar("INSERT INTO store_settings(name, cnpj) VALUES ($1, $2) RETURNING id::text")
        .bind(format!("Loja {tag}"))
        .bind(cnpj)
        .fetch_one(p)
        .await
        .unwrap()
}

fn app_with_db(p: sqlx::PgPool) -> axum::Router {
    app_router(AppState {
        pool: Some(p),
        jwt: JwtKeys::from_secret(TEST_SECRET).unwrap(),
        expose_docs: true,
        cert_master_key: TEST_CERT_KEY,
    })
}

async fn token_for(p: &sqlx::PgPool, store: &str, role: UserRole) -> String {
    let email = uniq("s91@loja");
    create_user(
        &PgUserRepository::new(p.clone()),
        store.parse::<Uuid>().unwrap(),
        CreateUserInput {
            name: "F".into(),
            email: email.clone(),
            password: "segredo-123".into(),
            role,
        },
    )
    .await
    .unwrap();
    let res = app_with_db(p.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("X-Store-ID", store)
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"email": email, "password": "segredo-123"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    json["token"].as_str().unwrap().to_string()
}

async fn call(
    p: &sqlx::PgPool,
    token: Option<&str>,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    let req = if let Some(b) = body {
        builder
            .header("Content-Type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    let res = app_with_db(p.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 256 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

/// Gera um PKCS#12 real em runtime (self-signed A1): nada de fixture `.pfx`
/// versionada (o `.gitignore` bloqueia e não haveria como rotacionar a senha).
fn make_pfx(password: &str, cnpj_in_cn: &str) -> Vec<u8> {
    let rsa = Rsa::generate(2048).unwrap();
    let pkey = PKey::from_rsa(rsa).unwrap();
    let mut name = X509NameBuilder::new().unwrap();
    name.append_entry_by_nid(Nid::COMMONNAME, &format!("LOJA TESTE LTDA:{cnpj_in_cn}"))
        .unwrap();
    let name = name.build();
    let mut builder = X509::builder().unwrap();
    builder.set_version(2).unwrap();
    let mut serial = BigNum::new().unwrap();
    serial.rand(128, MsbOption::MAYBE_ZERO, false).unwrap();
    let serial = serial.to_asn1_integer().unwrap();
    builder.set_serial_number(&serial).unwrap();
    builder.set_subject_name(&name).unwrap();
    builder.set_issuer_name(&name).unwrap();
    builder.set_pubkey(&pkey).unwrap();
    builder
        .set_not_before(&Asn1Time::days_from_now(0).unwrap())
        .unwrap();
    builder
        .set_not_after(&Asn1Time::days_from_now(365).unwrap())
        .unwrap();
    builder.sign(&pkey, MessageDigest::sha256()).unwrap();
    let cert = builder.build();
    Pkcs12::builder()
        .name("pdv-teste")
        .pkey(&pkey)
        .cert(&cert)
        .build2(password)
        .unwrap()
        .to_der()
        .unwrap()
}

fn payload(cnpj: &str, password: &str, file_name: &str, pfx: &[u8]) -> Value {
    json!({
        "cnpj": cnpj,
        "password": password,
        "file_name": file_name,
        "file_base64": STANDARD.encode(pfx),
    })
}

async fn row_of(
    p: &sqlx::PgPool,
    store: &str,
) -> (String, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, String, String) {
    sqlx::query_as(
        "SELECT cnpj, pfx_nonce, pfx_ciphertext, password_nonce, password_ciphertext,
                subject_cn, serial_hex
         FROM store_certificate WHERE store_id = $1::uuid",
    )
    .bind(store)
    .fetch_one(p)
    .await
    .unwrap()
}

#[tokio::test]
async fn upload_happy_cifra_em_repouso_e_audita_sem_segredos() {
    let p = pool().await;
    let (store, cnpj) = mk_store(&p, &uniq("h91")).await;
    let admin = token_for(&p, &store, UserRole::Admin).await;
    let password = "senha-super-secreta-91";
    let pfx = make_pfx(password, &cnpj);

    let (status, body) = call(
        &p,
        Some(&admin),
        "POST",
        routes::STORE_CERTIFICATE,
        Some(payload(&cnpj, password, "certificado.pfx", &pfx)),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["store_id"], store);
    assert_eq!(body["cnpj"], cnpj);
    assert!(body["subject_cn"].as_str().unwrap().contains(&cnpj));
    assert!(!body["serial_hex"].as_str().unwrap().is_empty());
    assert!(body["not_before"].is_string());
    assert!(body["not_after"].is_string());
    // Security case: resposta nunca devolve senha nem PFX.
    assert!(body.get("password").is_none());
    assert!(body.get("file_base64").is_none());
    assert!(body.get("pfx").is_none());
    assert!(!body.to_string().contains(password));

    // Banco: senha e PFX cifrados, decifráveis só com a chave mestra.
    let (db_cnpj, pfx_nonce, pfx_ct, pw_nonce, pw_ct, subject_cn, _serial) =
        row_of(&p, &store).await;
    assert_eq!(db_cnpj, cnpj);
    assert_eq!(pfx_nonce.len(), 12);
    assert_eq!(pw_nonce.len(), 12);
    assert!(
        !pw_ct
            .windows(password.len())
            .any(|w| w == password.as_bytes())
    );
    assert_ne!(pfx_ct, pfx);
    assert!(subject_cn.contains(&cnpj));
    let store_uuid = store.parse::<Uuid>().unwrap();
    let cipher = CertificateCipher::from_key(TEST_CERT_KEY);
    let decrypted_pw = cipher
        .decrypt(store_uuid, "password", &pw_nonce, &pw_ct)
        .unwrap();
    assert_eq!(decrypted_pw.as_slice(), password.as_bytes());
    let decrypted_pfx = cipher
        .decrypt(store_uuid, "pfx", &pfx_nonce, &pfx_ct)
        .unwrap();
    assert_eq!(decrypted_pfx.as_slice(), pfx.as_slice());

    // Auditoria: gravada e sem senha/PFX.
    let (action, entity, new_data): (String, String, Value) = sqlx::query_as(
        "SELECT action, entity, new_data FROM audit_log
         WHERE store_id = $1::uuid AND action = 'certificate.uploaded'
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&store)
    .fetch_one(&p)
    .await
    .unwrap();
    assert_eq!(action, "certificate.uploaded");
    assert_eq!(entity, "store_certificate");
    assert_eq!(new_data["cnpj"], cnpj);
    let raw = new_data.to_string();
    assert!(!raw.contains(password));
    assert!(!raw.contains(&STANDARD.encode(&pfx)));

    // GET devolve só metadados.
    let (status, body) = call(&p, Some(&admin), "GET", routes::STORE_CERTIFICATE, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["cnpj"], cnpj);
    assert!(body.get("password").is_none());
    assert!(!body.to_string().contains(password));
}

#[tokio::test]
async fn rbac_admin_only_e_sem_token_401() {
    let p = pool().await;
    let (store, cnpj) = mk_store(&p, &uniq("r91")).await;
    let pfx = make_pfx("senha-91", &cnpj);
    let body = payload(&cnpj, "senha-91", "certificado.p12", &pfx);

    let (status, _) = call(
        &p,
        None,
        "POST",
        routes::STORE_CERTIFICATE,
        Some(body.clone()),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    for role in [UserRole::Manager, UserRole::Cashier] {
        let token = token_for(&p, &store, role).await;
        let (status, json) = call(
            &p,
            Some(&token),
            "POST",
            routes::STORE_CERTIFICATE,
            Some(body.clone()),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{json}");
        let (status, _) = call(&p, Some(&token), "GET", routes::STORE_CERTIFICATE, None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    // Nada foi persistido pelos papéis barrados.
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM store_certificate WHERE store_id = $1::uuid")
            .bind(&store)
            .fetch_one(&p)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn validacoes_de_dominio_retornam_422() {
    let p = pool().await;
    let (store, cnpj) = mk_store(&p, &uniq("v91")).await;
    let admin = token_for(&p, &store, UserRole::Admin).await;
    let password = "senha-correta-91";
    let pfx = make_pfx(password, &cnpj);
    let other_cnpj = next_cnpj();

    let cases: Vec<(&str, Value)> = vec![
        (
            "extensão inválida",
            payload(&cnpj, password, "certificado.txt", &pfx),
        ),
        (
            "base64 inválido",
            payload(&cnpj, password, "certificado.pfx", b"???"),
        ),
        (
            "arquivo vazio",
            payload(&cnpj, password, "certificado.pfx", b""),
        ),
        (
            "arquivo acima de 5 MB",
            payload(
                &cnpj,
                password,
                "certificado.pfx",
                &vec![0u8; 5 * 1024 * 1024 + 1],
            ),
        ),
        ("senha vazia", payload(&cnpj, "", "certificado.pfx", &pfx)),
        (
            "senha acima de 128 chars",
            payload(&cnpj, &"x".repeat(129), "certificado.pfx", &pfx),
        ),
        (
            "CNPJ com pontuação",
            payload("11.222.333/0001-81", password, "certificado.pfx", &pfx),
        ),
        (
            "CNPJ com DV inválido",
            payload("11222333000182", password, "certificado.pfx", &pfx),
        ),
        (
            "CNPJ diferente do cadastro",
            payload(&other_cnpj, password, "certificado.pfx", &pfx),
        ),
        (
            "senha errada",
            payload(&cnpj, "senha-errada-91", "certificado.pfx", &pfx),
        ),
        (
            "arquivo corrompido",
            payload(&cnpj, password, "certificado.pfx", &[1, 2, 3, 4, 5]),
        ),
    ];
    for (label, body) in cases {
        let (status, json) = call(
            &p,
            Some(&admin),
            "POST",
            routes::STORE_CERTIFICATE,
            Some(body),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{label}: {json}");
        assert_eq!(json["code"], "UNPROCESSABLE", "{label}");
    }
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM store_certificate WHERE store_id = $1::uuid")
            .bind(&store)
            .fetch_one(&p)
            .await
            .unwrap();
    assert_eq!(count, 0, "nenhuma validação pode persistir certificado");
}

#[tokio::test]
async fn cnpj_do_certificado_precisa_bater_com_o_informado() {
    let p = pool().await;
    let (store, cnpj) = mk_store(&p, &uniq("c91")).await;
    let admin = token_for(&p, &store, UserRole::Admin).await;
    let cert_cnpj = next_cnpj();
    let pfx = make_pfx("senha-91", &cert_cnpj);

    let (status, json) = call(
        &p,
        Some(&admin),
        "POST",
        routes::STORE_CERTIFICATE,
        Some(payload(&cnpj, "senha-91", "certificado.pfx", &pfx)),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{json}");
    assert!(
        json["message"]
            .as_str()
            .unwrap()
            .contains("CNPJ do certificado")
    );
}

#[tokio::test]
async fn cadastro_com_cnpj_invalido_bloqueia_upload() {
    let p = pool().await;
    let store = mk_store_with_cnpj(&p, &uniq("l91"), &uniq("cnpj-legado")).await;
    let admin = token_for(&p, &store, UserRole::Admin).await;
    let cnpj = next_cnpj();
    let pfx = make_pfx("senha-91", &cnpj);

    let (status, json) = call(
        &p,
        Some(&admin),
        "POST",
        routes::STORE_CERTIFICATE,
        Some(payload(&cnpj, "senha-91", "certificado.pfx", &pfx)),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{json}");
    assert!(
        json["message"]
            .as_str()
            .unwrap()
            .contains("CNPJ da loja no cadastro")
    );
}

#[tokio::test]
async fn reupload_substitui_e_audita_metadados_anteriores() {
    let p = pool().await;
    let (store, cnpj) = mk_store(&p, &uniq("u91")).await;
    let admin = token_for(&p, &store, UserRole::Admin).await;

    let (status, first) = call(
        &p,
        Some(&admin),
        "POST",
        routes::STORE_CERTIFICATE,
        Some(payload(
            &cnpj,
            "senha-primeira-91",
            "certificado.pfx",
            &make_pfx("senha-primeira-91", &cnpj),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{first}");

    let (status, second) = call(
        &p,
        Some(&admin),
        "POST",
        routes::STORE_CERTIFICATE,
        Some(payload(
            &cnpj,
            "senha-segunda-91",
            "certificado.p12",
            &make_pfx("senha-segunda-91", &cnpj),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{second}");
    assert_ne!(first["serial_hex"], second["serial_hex"]);

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM store_certificate WHERE store_id = $1::uuid")
            .bind(&store)
            .fetch_one(&p)
            .await
            .unwrap();
    assert_eq!(count, 1, "upload é upsert (1:1 com a loja)");

    let rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log
         WHERE store_id = $1::uuid AND action = 'certificate.uploaded'",
    )
    .bind(&store)
    .fetch_one(&p)
    .await
    .unwrap();
    assert_eq!(rows, 2);
    let old_data: Option<Value> = sqlx::query_scalar(
        "SELECT old_data FROM audit_log
         WHERE store_id = $1::uuid AND action = 'certificate.uploaded'
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(&store)
    .fetch_one(&p)
    .await
    .unwrap();
    assert_eq!(old_data.unwrap()["serial_hex"], first["serial_hex"]);
}

#[tokio::test]
async fn isolamento_entre_lojas_no_get_e_no_upload() {
    let p = pool().await;
    let (store_a, cnpj_a) = mk_store(&p, &uniq("a91")).await;
    let (store_b, cnpj_b) = mk_store(&p, &uniq("b91")).await;
    let admin_a = token_for(&p, &store_a, UserRole::Admin).await;
    let admin_b = token_for(&p, &store_b, UserRole::Admin).await;

    let (status, _) = call(
        &p,
        Some(&admin_a),
        "POST",
        routes::STORE_CERTIFICATE,
        Some(payload(
            &cnpj_a,
            "senha-a-91",
            "certificado.pfx",
            &make_pfx("senha-a-91", &cnpj_a),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // B ainda não tem certificado: 404 (nunca vê o da A).
    let (status, json) = call(&p, Some(&admin_b), "GET", routes::STORE_CERTIFICATE, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{json}");

    // B envia o próprio: A permanece intacto.
    let (status, _) = call(
        &p,
        Some(&admin_b),
        "POST",
        routes::STORE_CERTIFICATE,
        Some(payload(
            &cnpj_b,
            "senha-b-91",
            "certificado.pfx",
            &make_pfx("senha-b-91", &cnpj_b),
        )),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (_, _, _, _, _, _, serial_a) = row_of(&p, &store_a).await;
    let (status, body) = call(&p, Some(&admin_a), "GET", routes::STORE_CERTIFICATE, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["serial_hex"], serial_a);
    assert_eq!(body["cnpj"], cnpj_a);
}

#[tokio::test]
async fn swagger_documenta_rota_schemas_e_erros() {
    let p = pool().await;
    let res = app_with_db(p)
        .oneshot(
            Request::builder()
                .uri(routes::OPENAPI_JSON)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 2 * 1024 * 1024).await.unwrap();
    let spec: Value = serde_json::from_slice(&bytes).unwrap();

    let item = &spec["paths"][routes::STORE_CERTIFICATE];
    for method in ["post", "get"] {
        assert!(
            item.get(method).is_some(),
            "método {method} ausente no spec"
        );
    }
    let post = &item["post"];
    assert!(post["requestBody"].is_object());
    for code in ["200", "401", "403", "413", "422"] {
        assert!(post["responses"].get(code).is_some(), "POST sem {code}");
    }
    assert!(post["security"].to_string().contains("bearer"));
    for code in ["200", "401", "403", "404"] {
        assert!(
            item["get"]["responses"].get(code).is_some(),
            "GET sem {code}"
        );
    }
    let schemas = spec["components"]["schemas"].as_object().unwrap();
    for name in ["UploadCertificateRequest", "CertificateResponse"] {
        assert!(schemas.contains_key(name), "schema {name} ausente");
    }
    // Nenhum schema/segredo interno pode vazar no spec.
    let raw = spec.to_string();
    for forbidden in ["password_ciphertext", "pfx_ciphertext", "CERT_MASTER_KEY"] {
        assert!(!raw.contains(forbidden), "spec vazou {forbidden}");
    }
}
