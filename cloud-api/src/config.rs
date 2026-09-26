//! Config via ambiente (.env no dev, env real em prod).

pub struct Config {
    pub database_url: String,
    pub port: u16,
    /// Segredo HMAC do JWT (US02). Validado em tamanho no boot (`JwtKeys`).
    pub jwt_secret: String,
    /// `development` | `test` | `production`. Docs OpenAPI só fora de produção.
    pub app_env: String,
}

/// Default APENAS dev local. Produção exige `JWT_SECRET` forte via ambiente.
const DEV_JWT_SECRET: &str = "dev-only-insecure-secret-troque-em-prod";

impl Config {
    pub fn from_env() -> Self {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://pdv:pdv@localhost:5432/pdv".to_string());
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3000);
        let jwt_secret = std::env::var("JWT_SECRET").unwrap_or_else(|_| DEV_JWT_SECRET.to_string());
        // Fail-closed: sem APP_ENV explícito, comporta-se como produção (docs off).
        let app_env = std::env::var("APP_ENV").unwrap_or_else(|_| "production".to_string());
        Self {
            database_url,
            port,
            jwt_secret,
            app_env,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Todos os caminhos de from_env em UMA função: testes no mesmo processo
    /// compartilham env, então paralelizar aqui causaria race entre set/remove.
    #[test]
    fn from_env_all_paths_sequential() {
        let saved_db = std::env::var("DATABASE_URL").ok();
        let saved_port = std::env::var("PORT").ok();
        let saved_env = std::env::var("APP_ENV").ok();

        // Rust 2024: manipular env é unsafe (race entre threads) — por isso
        // todos os casos vivem nesta única função, executada em sequência.
        unsafe {
            // 1. Defaults (nada no ambiente).
            std::env::remove_var("DATABASE_URL");
            std::env::remove_var("PORT");
            std::env::remove_var("APP_ENV");
        }
        let cfg = Config::from_env();
        assert_eq!(cfg.database_url, "postgres://pdv:pdv@localhost:5432/pdv");
        assert_eq!(cfg.port, 3000);
        // Fail-closed: sem APP_ENV, docs se comportam como produção (off).
        assert_eq!(cfg.app_env, "production");

        // 2. Valores customizados válidos.
        unsafe {
            std::env::set_var("DATABASE_URL", "postgres://u:p@db:5432/x");
            std::env::set_var("PORT", "8080");
        }
        let cfg = Config::from_env();
        assert_eq!(cfg.database_url, "postgres://u:p@db:5432/x");
        assert_eq!(cfg.port, 8080);

        // 3. PORT inválida (texto) → fallback 3000, sem panic.
        unsafe {
            std::env::set_var("PORT", "oitenta");
        }
        let cfg = Config::from_env();
        assert_eq!(cfg.port, 3000);

        // 4. PORT vazia → fallback 3000.
        unsafe {
            std::env::set_var("PORT", "");
        }
        let cfg = Config::from_env();
        assert_eq!(cfg.port, 3000);

        // 5. PORT fora da faixa u16 → fallback 3000.
        unsafe {
            std::env::set_var("PORT", "99999");
        }
        let cfg = Config::from_env();
        assert_eq!(cfg.port, 3000);

        // 6. APP_ENV respeitado (docs ligam fora de produção).
        unsafe {
            std::env::set_var("APP_ENV", "development");
        }
        let cfg = Config::from_env();
        assert_eq!(cfg.app_env, "development");

        // Restaura o ambiente como estava.
        unsafe {
            match saved_db {
                Some(v) => std::env::set_var("DATABASE_URL", v),
                None => std::env::remove_var("DATABASE_URL"),
            }
            match saved_port {
                Some(v) => std::env::set_var("PORT", v),
                None => std::env::remove_var("PORT"),
            }
            match saved_env {
                Some(v) => std::env::set_var("APP_ENV", v),
                None => std::env::remove_var("APP_ENV"),
            }
        }
    }
}
