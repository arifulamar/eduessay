use actix_cors::Cors;
use actix_web::{delete, get, middleware, patch, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use chrono::{Datelike, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use once_cell::sync::Lazy;
use sqlx::Row;
use std::collections::HashMap;
use tokio::sync::RwLock;
use sha2::{Digest, Sha256};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{MySql, MySqlPool};
use std::collections::HashSet;
use std::env;
use tracing::{error, info};
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    db: MySqlPool,
    http: reqwest::Client,
}

#[derive(Serialize)]
struct HealthResponse {
    ok: bool,
    database: &'static str,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PengajuanInput {
    nama: String,
    tempat_lahir: String,
    tanggal_lahir: String,
    nisn: String,
    tanggal_pengajuan: String,
    #[serde(default)]
    kelas: String,
    #[serde(default)]
    nama_ortu: String,
    #[serde(default)]
    alamat: String,
    #[serde(default)]
    tahun_lulus: String,
    #[serde(default)]
    jenis: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct Surat {
    jenis: String,
    status: String,
    nomor: String,
    catatan: String,
    tanggal_terbit: String,
}

#[derive(Serialize)]
struct StatusItem {
    jenis: String,
    status: String,
    catatan: String,
}

fn clean(value: &str, max: usize) -> String {
    value.trim().chars().take(max).collect()
}

fn valid_date(value: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()
}

fn error_json(status: actix_web::http::StatusCode, message: &str) -> HttpResponse {
    HttpResponse::build(status).json(serde_json::json!({ "error": message }))
}

fn required_env(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("Konfigurasi {} belum diisi", name))
}

fn mysql_options() -> Result<sqlx::mysql::MySqlConnectOptions, String> {
    let host = env::var("DB_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = env::var("DB_PORT").unwrap_or_else(|_| "3306".to_string())
        .parse::<u16>().map_err(|_| "DB_PORT harus berupa angka".to_string())?;
    let user = env::var("DB_USER").unwrap_or_else(|_| "educonnect".to_string());
    let password = required_env("DB_PASSWORD")?;
    let database = env::var("DB_NAME").unwrap_or_else(|_| "educonnect_tu".to_string());

    Ok(sqlx::mysql::MySqlConnectOptions::new()
        .host(&host).port(port).username(&user).password(&password)
        .database(&database).charset("utf8mb4"))
}

#[get("/health")]
