//! Config via ambiente (.env no dev, env real em prod).

pub struct Config {
    pub database_url: String,
    pub port: u16,
    /// Segredo HMAC do JWT (US02). Validado em tamanho no boot (`JwtKeys`).
    pub jwt_secret: String,
    /// `development` | `test` | `production`. Docs OpenAPI só fora de produção.
    pub app_env: String,
}

/// Fallback APENAS fora de produção. O segredo é público no repositório
/// (`.env.example`), então usá-lo em produção permitiria forjar qualquer JWT.
const DEV_JWT_SECRET: &str = "dev-only-insecure-secret-troque-em-prod";

impl Config {
    /// Fail-closed: sem `JWT_SECRET` e com `APP_ENV=production` (ou ausente,
    /// que assume produção), o boot deve ser recusado — nunca subir com
    /// segredo forjável. O fallback inseguro só vale em `development`/`test`.
    pub fn from_env() -> Result<Self, String> {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://pdv:pdv@localhost:5432/pdv".to_string());
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3000);
        // Fail-closed: sem APP_ENV explícito, comporta-se como produção (docs off).
        let app_env = std::env::var("APP_ENV").unwrap_or_else(|_| "production".to_string());
        let non_prod = matches!(app_env.as_str(), "development" | "test");
        let jwt_secret = match std::env::var("JWT_SECRET") {
            Ok(secret) => secret,
            Err(_) if non_prod => DEV_JWT_SECRET.to_string(),
            Err(_) => {
                return Err(
                    "JWT_SECRET ausente com APP_ENV=production: defina um segredo HMAC \
                     com ao menos 32 caracteres (nunca suba em produção com o \
                     fallback de desenvolvimento)"
                        .to_string(),
                );
            }
        };
        Ok(Self {
            database_url,
            port,
            jwt_secret,
            app_env,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Manipulação de env SÓ para o teste abaixo (Rust 2024: `set_var`/
    /// `remove_var` são `unsafe` — o SO não garante leitura thread-safe do
    /// ambiente, ver `std::env::set_var`).
    /// Auditoria: estes 2 helpers são o ÚNICO ponto do workspace que toca env
    /// em teste; o único chamador é uma função sequencial (sem acesso
    /// concorrente a ESTAS vars) com restore garantido no fim. A supressão
    /// `nosemgrep` vai TRAILING na linha do `unsafe {` — mesma linha do bloco
    /// e da chamada, onde qualquer finding ancora. Risco residual (leitores
    /// internos da std em outras threads) aceito: código só de teste.
    fn set_test_var(key: &str, value: &str) {
        unsafe { std::env::set_var(key, value) } // nosemgrep
    }
    fn remove_test_var(key: &str) {
        unsafe { std::env::remove_var(key) } // nosemgrep
    }

    /// Todos os caminhos de from_env em UMA função: testes no mesmo processo
    /// compartilham env, então paralelizar aqui causaria race entre set/remove.
    #[test]
    fn from_env_all_paths_sequential() {
        let saved_db = std::env::var("DATABASE_URL").ok();
        let saved_port = std::env::var("PORT").ok();
        let saved_env = std::env::var("APP_ENV").ok();
        let saved_jwt = std::env::var("JWT_SECRET").ok();
        // Segredo válido p/ os casos que precisam passar do gate fail-closed.
        const JWT_OK: &str = "segredo-valido-de-teste-com-32-chars!!";

        // Todos os casos vivem nesta única função, executada em sequência,
        // usando os helpers auditados acima (sem `unsafe` espalhado).
        // 1. Nada no ambiente → fail-closed (sem APP_ENV assume produção,
        // sem JWT recusa o boot em vez de usar o fallback forjável).
        remove_test_var("DATABASE_URL");
        remove_test_var("PORT");
        remove_test_var("APP_ENV");
        remove_test_var("JWT_SECRET");
        // Sem `Debug` em `Config` de propósito (carrega o segredo): `match`
        // em vez de `expect_err`, que exigiria o trait.
        let err = match Config::from_env() {
            Err(e) => e,
            Ok(_) => panic!("boot sem JWT em produção deve falhar"),
        };
        assert!(err.contains("JWT_SECRET"), "erro deve apontar o JWT: {err}");

        // 2. Valores customizados válidos (JWT explícito + APP_ENV ausente
        // continua assumindo produção, mas com segredo próprio está ok).
        set_test_var("DATABASE_URL", "postgres://u:p@db:5432/x");
        set_test_var("PORT", "8080");
        set_test_var("JWT_SECRET", JWT_OK);
        let cfg = Config::from_env().expect("JWT explícito deve passar");
        assert_eq!(cfg.database_url, "postgres://u:p@db:5432/x");
        assert_eq!(cfg.port, 8080);
        // Fail-closed: sem APP_ENV, docs se comportam como produção (off).
        assert_eq!(cfg.app_env, "production");
        assert_eq!(cfg.jwt_secret, JWT_OK);

        // 3. PORT inválida (texto) → fallback 3000, sem panic.
        set_test_var("PORT", "oitenta");
        let cfg = Config::from_env().expect("PORT inválida não pode falhar o boot");
        assert_eq!(cfg.port, 3000);

        // 4. PORT vazia → fallback 3000.
        set_test_var("PORT", "");
        let cfg = Config::from_env().expect("PORT vazia não pode falhar o boot");
        assert_eq!(cfg.port, 3000);

        // 5. PORT fora da faixa u16 → fallback 3000.
        set_test_var("PORT", "99999");
        let cfg = Config::from_env().expect("PORT fora da faixa não pode falhar o boot");
        assert_eq!(cfg.port, 3000);

        // 6. APP_ENV respeitado (docs ligam fora de produção).
        set_test_var("APP_ENV", "development");
        let cfg = Config::from_env().expect("JWT explícito deve passar em dev");
        assert_eq!(cfg.app_env, "development");
        assert_eq!(cfg.jwt_secret, JWT_OK);

        // 7. development SEM jwt → fallback inseguro permitido (só fora de prod).
        remove_test_var("JWT_SECRET");
        let cfg = Config::from_env().expect("dev sem JWT usa o fallback");
        assert_eq!(cfg.jwt_secret, DEV_JWT_SECRET);

        // 8. test SEM jwt → fallback permitido.
        set_test_var("APP_ENV", "test");
        let cfg = Config::from_env().expect("test sem JWT usa o fallback");
        assert_eq!(cfg.jwt_secret, DEV_JWT_SECRET);

        // 9. production explícito SEM jwt → recusa.
        set_test_var("APP_ENV", "production");
        let err = match Config::from_env() {
            Err(e) => e,
            Ok(_) => panic!("produção sem JWT deve falhar"),
        };
        assert!(err.contains("JWT_SECRET"), "erro deve apontar o JWT: {err}");

        // 10. production COM jwt → ok.
        set_test_var("JWT_SECRET", JWT_OK);
        let cfg = Config::from_env().expect("produção com JWT deve passar");
        assert_eq!(cfg.app_env, "production");
        assert_eq!(cfg.jwt_secret, JWT_OK);

        // Restaura o ambiente como estava.
        match saved_db {
            Some(v) => set_test_var("DATABASE_URL", &v),
            None => remove_test_var("DATABASE_URL"),
        }
        match saved_port {
            Some(v) => set_test_var("PORT", &v),
            None => remove_test_var("PORT"),
        }
        match saved_env {
            Some(v) => set_test_var("APP_ENV", &v),
            None => remove_test_var("APP_ENV"),
        }
        match saved_jwt {
            Some(v) => set_test_var("JWT_SECRET", &v),
            None => remove_test_var("JWT_SECRET"),
        }
    }
}
