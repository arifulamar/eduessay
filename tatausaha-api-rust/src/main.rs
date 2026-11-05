use actix_cors::Cors;
use actix_web::{get, middleware, post, web, App, HttpResponse, HttpServer, Responder};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
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
async fn health(state: web::Data<AppState>) -> impl Responder {
    match sqlx::query_scalar::<MySql, i32>("SELECT 1").fetch_one(&state.db).await {
        Ok(_) => HttpResponse::Ok().json(HealthResponse { ok: true, database: "connected" }),
        Err(err) => {
            error!("Health check database gagal: {}", err);
            error_json(actix_web::http::StatusCode::SERVICE_UNAVAILABLE, "Database belum terhubung")
        }
    }
}

#[post("/api/pengajuan")]
async fn create_pengajuan(state: web::Data<AppState>, body: web::Json<PengajuanInput>) -> HttpResponse {
    let b = body.into_inner();
    let nama = clean(&b.nama, 120);
    let tempat_lahir = clean(&b.tempat_lahir, 100);
    let tanggal_lahir = clean(&b.tanggal_lahir, 10);
    let nisn = clean(&b.nisn, 10);
    let tanggal_pengajuan = clean(&b.tanggal_pengajuan, 10);
    let kelas = clean(&b.kelas, 40);
    let nama_ortu = clean(&b.nama_ortu, 120);
    let alamat = clean(&b.alamat, 500);
    let tahun_lulus = clean(&b.tahun_lulus, 9);

    let mut jenis_set = HashSet::new();
    let jenis: Vec<String> = b.jenis.into_iter().filter(|j| jenis_set.insert(j.clone())).collect();
    if jenis.is_empty() || jenis.len() > 2 || jenis.iter().any(|j| j != "aktif" && j != "skhun") {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Pilih jenis surat yang valid");
    }

    let Some(lahir) = valid_date(&tanggal_lahir) else {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Lengkapi identitas dan NISN 10 digit");
    };
    let Some(pengajuan) = valid_date(&tanggal_pengajuan) else {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Lengkapi identitas dan NISN 10 digit");
    };

    if nama.is_empty() || tempat_lahir.is_empty() || !nisn.chars().all(|c| c.is_ascii_digit()) || nisn.len() != 10 {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Lengkapi identitas dan NISN 10 digit");
    }
    if jenis.iter().any(|j| j == "aktif") && kelas.is_empty() {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Kelas wajib diisi");
    }
    if jenis.iter().any(|j| j == "skhun") {
        let valid_tahun = tahun_lulus.split_once('/').map(|(a,b)| {
            a.len() == 4 && b.len() == 4 &&
            a.parse::<i32>().is_ok_and(|y| b.parse::<i32>().is_ok_and(|z| z == y + 1))
        }).unwrap_or(false);
        if nama_ortu.is_empty() || alamat.is_empty() || !valid_tahun {
            return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Nama orang tua, alamat, dan tahun pelajaran lulus wajib diisi");
        }
    }
    let today = Utc::now().date_naive();
    if lahir > today || pengajuan > today.succ_opt().unwrap_or(today) {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Tanggal tidak valid");
    }

    let id = Uuid::new_v4().to_string();
    let kode = Uuid::new_v4().simple().to_string().to_uppercase();
    let hash = format!("{:x}", Sha256::digest(kode.as_bytes()));
    let surat: Vec<Surat> = jenis.into_iter().map(|j| Surat {
        jenis: j, status: "menunggu".to_string(), nomor: String::new(),
        catatan: String::new(), tanggal_terbit: String::new()
    }).collect();
    let surat_json = match serde_json::to_string(&surat) {
        Ok(value) => value,
        Err(err) => {
            error!("Serialisasi surat gagal: {}", err);
            return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal menyimpan pengajuan");
        }
    };

    let result = sqlx::query(
        "INSERT INTO tu_pengajuan (id,nama,tempat_lahir,tanggal_lahir,nisn,tanggal_pengajuan,kelas,nama_ortu,alamat,tahun_lulus,kode_hash,surat) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)"
    )
    .bind(id).bind(nama).bind(tempat_lahir).bind(lahir).bind(nisn).bind(pengajuan)
    .bind(kelas).bind(nama_ortu).bind(alamat).bind(tahun_lulus).bind(hash).bind(surat_json)
    .execute(&state.db).await;

    match result {
        Ok(_) => HttpResponse::Created().json(serde_json::json!({ "kode": kode })),
        Err(err) => {
            error!("Gagal menyimpan pengajuan: {}", err);
            error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal menyimpan pengajuan")
        }
    }
}

#[get("/api/status/{kode}")]
async fn get_status(state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let kode = clean(&path.into_inner(), 64);
    if kode.len() != 32 || !kode.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()) {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Kode tidak valid");
    }
    let hash = format!("{:x}", Sha256::digest(kode.as_bytes()));
    let row = sqlx::query_scalar::<MySql, String>(
        "SELECT CAST(surat AS CHAR) FROM tu_pengajuan WHERE kode_hash=? LIMIT 1"
    ).bind(hash).fetch_optional(&state.db).await;

    match row {
        Ok(Some(raw)) => {
            let parsed: Vec<Surat> = match serde_json::from_str(&raw) {
                Ok(value) => value,
                Err(err) => {
                    error!("Data surat JSON tidak valid: {}", err);
                    return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal memeriksa status");
                }
            };
            let surat: Vec<StatusItem> = parsed.into_iter().map(|s| StatusItem {
                jenis: s.jenis, status: s.status, catatan: s.catatan
            }).collect();
            HttpResponse::Ok().json(serde_json::json!({ "surat": surat }))
        }
        Ok(None) => error_json(actix_web::http::StatusCode::NOT_FOUND, "Pengajuan tidak ditemukan"),
        Err(err) => {
            error!("Gagal memeriksa status: {}", err);
            error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal memeriksa status")
        }
    }
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt().with_env_filter(
        env::var("RUST_LOG").unwrap_or_else(|_| "info,actix_web=info,sqlx=warn".to_string())
    ).init();

    let options = mysql_options().map_err(std::io::Error::other)?;
    let pool_size = env::var("DB_POOL_SIZE").unwrap_or_else(|_| "2".to_string())
        .parse::<u32>().map_err(|_| std::io::Error::other("DB_POOL_SIZE harus berupa angka"))?;
    let db = MySqlPoolOptions::new().max_connections(pool_size.clamp(1, 5))
        .min_connections(0).acquire_timeout(std::time::Duration::from_secs(10))
        .connect_with(options).await.map_err(|err| {
            error!("Gagal menghubungkan MySQL: {}", err);
            std::io::Error::other("Koneksi database gagal; periksa konfigurasi DB pada .env")
        })?;
    sqlx::query("SELECT 1").execute(&db).await.map_err(|err| {
        error!("Tes awal database gagal: {}", err);
        std::io::Error::other("Tes database gagal")
    })?;

    let state = web::Data::new(AppState { db });
    let port = env::var("PORT").unwrap_or_else(|_| "8081".to_string())
        .parse::<u16>().map_err(|_| std::io::Error::other("PORT harus berupa angka"))?;
    let origins: Vec<String> = env::var("ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "https://educonnect.my.id".to_string())
        .split(',').map(str::trim).filter(|x| !x.is_empty()).map(ToOwned::to_owned).collect();

    info!("MySQL terhubung. Rust API akan berjalan pada 127.0.0.1:{}", port);
    HttpServer::new(move || {
        let mut cors = Cors::default().allowed_methods(vec!["GET","POST","PATCH","DELETE"])
            .allowed_headers(vec![actix_web::http::header::AUTHORIZATION, actix_web::http::header::CONTENT_TYPE])
            .max_age(3600);
        for origin in &origins { cors = cors.allowed_origin(origin); }
        App::new().app_data(state.clone())
            .app_data(web::JsonConfig::default().limit(20 * 1024))
            .wrap(cors).wrap(middleware::Compress::default())
            .wrap(middleware::Logger::default())
            .service(health).service(create_pengajuan).service(get_status)
    }).bind(("127.0.0.1", port))?.run().await
}
