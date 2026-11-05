use actix_cors::Cors;
use actix_web::{get, middleware, web, App, HttpResponse, HttpServer, Responder};
use serde::Serialize;
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{MySql, MySqlPool};
use std::env;
use tracing::{error, info};

#[derive(Clone)]
struct AppState {
    db: MySqlPool,
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
    database: &'static str,
}

fn required_env(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("Konfigurasi {} belum diisi", name))
}

fn mysql_options() -> Result<sqlx::mysql::MySqlConnectOptions, String> {
    let host = env::var("DB_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("DB_PORT")
        .unwrap_or_else(|_| "3306".to_string())
        .parse::<u16>()
        .map_err(|_| "DB_PORT harus berupa angka".to_string())?;
    let user = env::var("DB_USER").unwrap_or_else(|_| "educonnect".to_string());
    let password = required_env("DB_PASSWORD")?;
    let database = env::var("DB_NAME").unwrap_or_else(|_| "educonnect_tu".to_string());

    Ok(sqlx::mysql::MySqlConnectOptions::new()
        .host(&host)
        .port(port)
        .username(&user)
        .password(&password)
        .database(&database)
        .charset("utf8mb4"))
}

#[get("/health")]
async fn health(state: web::Data<AppState>) -> impl Responder {
    match sqlx::query_scalar::<MySql, i32>("SELECT 1")
        .fetch_one(&state.db)
        .await
    {
        Ok(_) => HttpResponse::Ok().json(HealthResponse {
            ok: true,
            database: "connected",
        }),
        Err(err) => {
            error!("Health check database gagal: {}", err);
            HttpResponse::ServiceUnavailable().json(serde_json::json!({
                "ok": false,
                "error": "Database belum terhubung"
            }))
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            env::var("RUST_LOG").unwrap_or_else(|_| "info,actix_web=info,sqlx=warn".to_string()),
        )
        .init();

    let options = mysql_options().map_err(std::io::Error::other)?;
    let pool_size = env::var("DB_POOL_SIZE")
        .unwrap_or_else(|_| "2".to_string())
        .parse::<u32>()
        .map_err(|_| std::io::Error::other("DB_POOL_SIZE harus berupa angka"))?;

    let db = MySqlPoolOptions::new()
        .max_connections(pool_size.clamp(1, 5))
        .min_connections(0)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect_with(options)
        .await
        .map_err(|err| {
            error!("Gagal menghubungkan MySQL: {}", err);
            std::io::Error::other("Koneksi database gagal; periksa konfigurasi DB pada .env")
        })?;

    sqlx::query("SELECT 1")
        .execute(&db)
        .await
        .map_err(|err| {
            error!("Tes awal database gagal: {}", err);
            std::io::Error::other("Tes database gagal")
        })?;

    let state = web::Data::new(AppState { db });
    let port = env::var("PORT")
        .unwrap_or_else(|_| "8081".to_string())
        .parse::<u16>()
        .map_err(|_| std::io::Error::other("PORT harus berupa angka"))?;

    let origins: Vec<String> = env::var("ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "https://educonnect.my.id".to_string())
        .split(',')
        .map(str::trim)
        .filter(|origin| !origin.is_empty())
        .map(ToOwned::to_owned)
        .collect();

    info!("MySQL terhubung. Rust API akan berjalan pada 127.0.0.1:{}", port);

    HttpServer::new(move || {
        let mut cors = Cors::default()
            .allowed_methods(vec!["GET", "POST", "PATCH", "DELETE"])
            .allowed_headers(vec![
                actix_web::http::header::AUTHORIZATION,
                actix_web::http::header::CONTENT_TYPE,
            ])
            .max_age(3600);

        for origin in &origins {
            cors = cors.allowed_origin(origin);
        }

        App::new()
            .app_data(state.clone())
            .app_data(web::JsonConfig::default().limit(20 * 1024))
            .wrap(cors)
            .wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default())
            .service(health)
    })
    .bind(("127.0.0.1", port))?
    .run()
    .await
}
