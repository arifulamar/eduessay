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
        "INSERT INTO tu_pengajuan (id,nama,tempat_lahir,tanggal_lahir,nisn,tanggal_pengajuan,kelas,nama_ortu,alamat,tahun_lulus,kode_hash,kode_pengajuan,surat) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)"
    )
    .bind(id).bind(nama).bind(tempat_lahir).bind(lahir).bind(nisn).bind(pengajuan)
    .bind(kelas).bind(nama_ortu).bind(alamat).bind(tahun_lulus).bind(hash).bind(&kode).bind(surat_json)
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


#[get("/api/status/{kode}/surat/{jenis}/pdf")]
async fn download_public_letter_pdf(
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    let (kode, jenis) = path.into_inner();
    let kode = clean(&kode, 64);
    if kode.len() != 32 || !kode.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_lowercase()) {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Kode pelacakan tidak valid");
    }
    if jenis != "aktif" && jenis != "skhun" {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Jenis surat tidak valid");
    }

    let hash = format!("{:x}", Sha256::digest(kode.as_bytes()));
    let row = sqlx::query(
        "SELECT id,nama,tempat_lahir,DATE_FORMAT(tanggal_lahir,'%Y-%m-%d') AS tanggal_lahir,nisn,DATE_FORMAT(tanggal_pengajuan,'%Y-%m-%d') AS tanggal_pengajuan,kelas,nama_ortu,alamat,tahun_lulus,CAST(surat AS CHAR) AS surat_json FROM tu_pengajuan WHERE kode_hash=? LIMIT 1"
    ).bind(hash).fetch_optional(&state.db).await;

    let row = match row {
        Ok(Some(row)) => row,
        Ok(None) => return error_json(actix_web::http::StatusCode::NOT_FOUND, "Pengajuan tidak ditemukan"),
        Err(err) => {
            error!("Gagal mengambil data surat publik: {}", err);
            return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal mengambil data surat");
        }
    };

    let raw: String = row.get("surat_json");
    let surat: Vec<Surat> = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(err) => {
            error!("Data surat publik tidak valid: {}", err);
            return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Data surat tidak valid");
        }
    };
    let Some(item) = surat.into_iter().find(|s| s.jenis == jenis) else {
        return error_json(actix_web::http::StatusCode::NOT_FOUND, "Jenis surat tidak diajukan");
    };
    if item.status != "diterbitkan" {
        return error_json(actix_web::http::StatusCode::FORBIDDEN, "Surat belum diterbitkan");
    }

    let data = serde_json::json!({
        "id": row.get::<String,_>("id"),
        "nama": row.get::<String,_>("nama"),
        "tempatLahir": row.get::<String,_>("tempat_lahir"),
        "tanggalLahir": row.get::<String,_>("tanggal_lahir"),
        "nisn": row.get::<String,_>("nisn"),
        "tanggalPengajuan": row.get::<String,_>("tanggal_pengajuan"),
        "kelas": row.try_get::<Option<String>,_>("kelas").ok().flatten().unwrap_or_default(),
        "namaOrtu": row.try_get::<Option<String>,_>("nama_ortu").ok().flatten().unwrap_or_default(),
        "alamat": row.try_get::<Option<String>,_>("alamat").ok().flatten().unwrap_or_default(),
        "tahunLulus": row.try_get::<Option<String>,_>("tahun_lulus").ok().flatten().unwrap_or_default()
    });

    match make_letter_pdf_chrome(&data, &item) {
        Ok(bytes) => HttpResponse::Ok()
            .insert_header(("Content-Type", "application/pdf"))
            .insert_header(("Content-Disposition", format!("attachment; filename=\"surat-{}-{}.pdf\"", jenis, data["nisn"].as_str().unwrap_or("siswa"))))
            .insert_header(("Cache-Control", "no-store"))
            .body(bytes),
        Err(err) => {
            error!("Gagal membuat PDF publik: {}", err);
            error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal membuat PDF surat")
        }
    }
}


#[derive(Deserialize)]
struct FirebaseClaims {
    aud: String,
    iss: String,
    exp: usize,
    sub: String,
    email: Option<String>,
    email_verified: Option<bool>,
}

static FIREBASE_CERTS: Lazy<RwLock<HashMap<String, String>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

#[derive(Clone)]
struct StaffIdentity {
    email: String,
    superadmin: bool,
}

async fn verify_firebase_token(token: &str, project_id: &str, client: &reqwest::Client) -> Result<FirebaseClaims, String> {
    let header = decode_header(token).map_err(|_| "Token login tidak valid".to_string())?;
    if header.alg != Algorithm::RS256 {
        return Err("Algoritma token tidak didukung".to_string());
    }
    let kid = header.kid.ok_or_else(|| "Token tidak memiliki key ID".to_string())?;
    {
        let cache = FIREBASE_CERTS.read().await;
        if !cache.contains_key(&kid) {
            drop(cache);
            let response = client.get("https://www.googleapis.com/robot/v1/metadata/x509/securetoken@system.gserviceaccount.com")
                .send().await.map_err(|_| "Tidak dapat memverifikasi layanan autentikasi".to_string())?;
            if !response.status().is_success() {
                return Err("Sertifikat autentikasi tidak tersedia".to_string());
            }
            let certs: HashMap<String, String> = response.json().await
                .map_err(|_| "Sertifikat autentikasi tidak valid".to_string())?;
            let mut cache = FIREBASE_CERTS.write().await;
            *cache = certs;
        }
    }
    let pem = FIREBASE_CERTS.read().await.get(&kid).cloned()
        .ok_or_else(|| "Kunci verifikasi token tidak ditemukan".to_string())?;
    let key = DecodingKey::from_rsa_pem(pem.as_bytes())
        .map_err(|_| "Kunci verifikasi token tidak valid".to_string())?;
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[project_id]);
    validation.set_issuer(&[format!("https://securetoken.google.com/{}", project_id)]);
    validation.validate_exp = true;
    let data = decode::<FirebaseClaims>(token, &key, &validation)
        .map_err(|_| "Sesi tidak valid atau sudah kedaluwarsa".to_string())?;
    if data.claims.sub.is_empty() || data.claims.email_verified != Some(true) || data.claims.email.is_none() {
        return Err("Email Google terverifikasi diperlukan".to_string());
    }
    Ok(data.claims)
}

async fn authorize(req: &HttpRequest, state: &AppState) -> Result<StaffIdentity, HttpResponse> {
    let token = req.headers().get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| error_json(actix_web::http::StatusCode::UNAUTHORIZED, "Login diperlukan"))?;
    let project = env::var("FIREBASE_PROJECT_ID").unwrap_or_else(|_| "eduessay-bd0ae".to_string());
    let claims = verify_firebase_token(token, &project, &state.http).await
        .map_err(|e| error_json(actix_web::http::StatusCode::UNAUTHORIZED, &e))?;
    let email = claims.email.unwrap_or_default().trim().to_lowercase();
    let superadmins: Vec<String> = env::var("SUPERADMIN_EMAILS")
        .unwrap_or_else(|_| "ariful.amar@gmail.com".to_string())
        .split(',').map(|x| x.trim().to_lowercase()).filter(|x| !x.is_empty()).collect();
    let superadmin = superadmins.contains(&email);
    if !superadmin {
        let exists = sqlx::query_scalar::<MySql, i64>("SELECT COUNT(*) FROM tu_admins WHERE email=?")
            .bind(&email).fetch_one(&state.db).await;
        match exists {
            Ok(count) if count > 0 => {},
            Ok(_) => return Err(error_json(actix_web::http::StatusCode::FORBIDDEN, "Email ini belum diizinkan sebagai staf TU")),
            Err(err) => {
                error!("Pemeriksaan hak staf gagal: {}", err);
                return Err(error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal memeriksa hak akses"));
            }
        }
    }
    Ok(StaffIdentity { email, superadmin })
}

#[get("/api/admin/me")]
async fn admin_me(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    match authorize(&req, &state).await {
        Ok(staff) => HttpResponse::Ok().json(serde_json::json!({
            "email": staff.email,
            "role": if staff.superadmin { "superadmin" } else { "staf" }
        })),
        Err(response) => response,
    }
}

#[get("/api/superadmin/staff")]
async fn list_staff(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    let staff = match authorize(&req, &state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !staff.superadmin {
        return error_json(actix_web::http::StatusCode::FORBIDDEN, "Khusus Superadmin");
    }
    let rows = sqlx::query("SELECT email, DATE_FORMAT(created_at, '%Y-%m-%d %H:%i:%s') AS added_at FROM tu_admins ORDER BY email")
        .fetch_all(&state.db).await;
    match rows {
        Ok(rows) => {
            let data: Vec<serde_json::Value> = rows.into_iter().map(|row| serde_json::json!({
                "email": row.get::<String, _>("email"),
                "addedAt": row.get::<String, _>("added_at")
            })).collect();
            HttpResponse::Ok().json(data)
        }
        Err(err) => {
            error!("Gagal memuat staf: {}", err);
            error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal memuat akun staf")
        }
    }
}

#[derive(Deserialize)]
struct StaffInput { email: String }

fn valid_email(email: &str) -> bool {
    let mut parts = email.split('@');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(local), Some(domain), None) => !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.'),
        _ => false
    }
}

#[post("/api/superadmin/staff")]
async fn add_staff(req: HttpRequest, state: web::Data<AppState>, body: web::Json<StaffInput>) -> HttpResponse {
    let staff = match authorize(&req, &state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !staff.superadmin {
        return error_json(actix_web::http::StatusCode::FORBIDDEN, "Khusus Superadmin");
    }
    let email = clean(&body.email, 254).to_lowercase();
    if !valid_email(&email) {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Alamat email tidak valid");
    }
    let superadmins: Vec<String> = env::var("SUPERADMIN_EMAILS")
        .unwrap_or_else(|_| "ariful.amar@gmail.com".to_string())
        .split(',').map(|x| x.trim().to_lowercase()).collect();
    if superadmins.contains(&email) {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Email Superadmin tidak perlu ditambahkan sebagai staf");
    }
    let result = sqlx::query("INSERT INTO tu_admins (email, added_by) VALUES (?,?) ON DUPLICATE KEY UPDATE added_by=VALUES(added_by)")
        .bind(&email).bind(&staff.email).execute(&state.db).await;
    match result {
        Ok(_) => HttpResponse::Created().json(serde_json::json!({"ok":true,"email":email})),
        Err(err) => {
            error!("Gagal menambah staf: {}", err);
            error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal menambahkan staf")
        }
    }
}

#[delete("/api/superadmin/staff/{email}")]
async fn delete_staff(req: HttpRequest, state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    let staff = match authorize(&req, &state).await {
        Ok(value) => value,
        Err(response) => return response,
    };
    if !staff.superadmin {
        return error_json(actix_web::http::StatusCode::FORBIDDEN, "Khusus Superadmin");
    }
    let email = clean(&path.into_inner(), 254).to_lowercase();
    let superadmins: Vec<String> = env::var("SUPERADMIN_EMAILS")
        .unwrap_or_else(|_| "ariful.amar@gmail.com".to_string())
        .split(',').map(|x| x.trim().to_lowercase()).collect();
    if email.is_empty() || superadmins.contains(&email) {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Tidak dapat menghapus akun Superadmin");
    }
    match sqlx::query("DELETE FROM tu_admins WHERE email=?").bind(email).execute(&state.db).await {
        Ok(_) => HttpResponse::Ok().json(serde_json::json!({"ok":true})),
        Err(err) => {
            error!("Gagal menghapus staf: {}", err);
            error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal menghapus akses staf")
        }
    }
}

#[delete("/api/admin/pengajuan/{id}")]
async fn delete_pengajuan(req: HttpRequest, state: web::Data<AppState>, path: web::Path<String>) -> HttpResponse {
    if let Err(response) = authorize(&req, &state).await {
        return response;
    }
    let id = clean(&path.into_inner(), 36);
    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            error!("Gagal memulai transaksi hapus pengajuan: {}", err);
            return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal menghapus pengajuan");
        }
    };
    if let Err(err) = sqlx::query("DELETE FROM tu_nomor_surat WHERE request_id=?").bind(&id).execute(&mut *tx).await {
        error!("Gagal menghapus relasi nomor surat: {}", err);
        let _ = tx.rollback().await;
        return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal menghapus riwayat nomor surat");
    }
    match sqlx::query("DELETE FROM tu_pengajuan WHERE id=?").bind(&id).execute(&mut *tx).await {
        Ok(result) if result.rows_affected() == 1 => {
            if let Err(err) = tx.commit().await {
                error!("Gagal menyimpan penghapusan pengajuan: {}", err);
                return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Penghapusan gagal disimpan");
            }
            HttpResponse::Ok().json(serde_json::json!({"ok":true}))
        }
        Ok(_) => {
            let _ = tx.rollback().await;
            error_json(actix_web::http::StatusCode::NOT_FOUND, "Pengajuan tidak ditemukan")
        }
        Err(err) => {
            error!("Gagal menghapus pengajuan: {}", err);
            let _ = tx.rollback().await;
            error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal menghapus pengajuan")
        }
    }
}

#[get("/api/admin/pengajuan")]
async fn list_pengajuan(req: HttpRequest, state: web::Data<AppState>) -> HttpResponse {
    if let Err(response) = authorize(&req, &state).await {
        return response;
    }
    let rows = sqlx::query(
        "SELECT id,nama,tempat_lahir,DATE_FORMAT(tanggal_lahir,'%Y-%m-%d') AS tanggal_lahir,nisn,DATE_FORMAT(tanggal_pengajuan,'%Y-%m-%d') AS tanggal_pengajuan,kelas,nama_ortu,alamat,tahun_lulus,kode_pengajuan,CAST(surat AS CHAR) AS surat_json FROM tu_pengajuan ORDER BY created_at DESC LIMIT 200"
    ).fetch_all(&state.db).await;
    match rows {
        Ok(rows) => {
            let mut data = Vec::with_capacity(rows.len());
            for row in rows {
                let raw: String = row.get("surat_json");
                let surat: serde_json::Value = match serde_json::from_str(&raw) {
                    Ok(value) => value,
                    Err(err) => {
                        error!("JSON surat tidak valid: {}", err);
                        return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Data surat tidak valid");
                    }
                };
                data.push(serde_json::json!({
                    "id": row.get::<String,_>("id"),
                    "nama": row.get::<String,_>("nama"),
                    "tempatLahir": row.get::<String,_>("tempat_lahir"),
                    "tanggalLahir": row.get::<String,_>("tanggal_lahir"),
                    "nisn": row.get::<String,_>("nisn"),
                    "tanggalPengajuan": row.get::<String,_>("tanggal_pengajuan"),
                    "kelas": row.try_get::<Option<String>,_>("kelas").ok().flatten().unwrap_or_default(),
                    "namaOrtu": row.try_get::<Option<String>,_>("nama_ortu").ok().flatten().unwrap_or_default(),
                    "alamat": row.try_get::<Option<String>,_>("alamat").ok().flatten().unwrap_or_default(),
                    "tahunLulus": row.try_get::<Option<String>,_>("tahun_lulus").ok().flatten().unwrap_or_default(),
                    "kodePengajuan": row.try_get::<Option<String>,_>("kode_pengajuan").ok().flatten().unwrap_or_default(),
                    "surat": surat
                }));
            }
            HttpResponse::Ok().json(data)
        }
        Err(err) => {
            error!("Gagal memuat pengajuan: {}", err);
            error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal memuat pengajuan")
        }
    }
}


#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessLetterInput {
    status: String,
    #[serde(default)]
    nomor: String,
    #[serde(default)]
    catatan: String,
}

#[patch("/api/admin/pengajuan/{id}/surat/{jenis}")]
async fn process_letter(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
    body: web::Json<ProcessLetterInput>,
) -> HttpResponse {
    let staff = match authorize(&req, &state).await {
        Ok(value) => value,
        Err(response) => return response,
    };

    let (id, jenis) = path.into_inner();
    if jenis != "aktif" && jenis != "skhun" {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Jenis surat tidak valid");
    }

    let status = clean(&body.status, 20);
    let nomor = clean(&body.nomor, 100);
    let catatan = clean(&body.catatan, 500);

    if !["perbaikan", "ditolak", "diterbitkan"].contains(&status.as_str()) {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Status tidak valid");
    }
    if status == "diterbitkan" && nomor.is_empty() {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Nomor surat wajib diisi");
    }
    if status == "perbaikan" && catatan.is_empty() {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Tuliskan catatan perbaikan");
    }

    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(err) => {
            error!("Gagal memulai transaksi penerbitan: {}", err);
            return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal memproses surat");
        }
    };

    let row = match sqlx::query("SELECT CAST(surat AS CHAR) AS surat_json FROM tu_pengajuan WHERE id=? FOR UPDATE")
        .bind(&id).fetch_optional(&mut *tx).await {
        Ok(Some(row)) => row,
        Ok(None) => {
            let _ = tx.rollback().await;
            return error_json(actix_web::http::StatusCode::NOT_FOUND, "Pengajuan tidak ditemukan");
        }
        Err(err) => {
            error!("Gagal membaca pengajuan untuk diproses: {}", err);
            let _ = tx.rollback().await;
            return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal memproses surat");
        }
    };

    let raw: String = row.get("surat_json");
    let mut surat: Vec<Surat> = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(err) => {
            error!("Data surat tidak valid: {}", err);
            let _ = tx.rollback().await;
            return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Data surat tidak valid");
        }
    };

    let Some(index) = surat.iter().position(|item| item.jenis == jenis) else {
        let _ = tx.rollback().await;
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Jenis surat tidak diajukan");
    };

    if surat[index].status == "diterbitkan" {
        let _ = tx.rollback().await;
        return error_json(actix_web::http::StatusCode::CONFLICT, "Surat sudah diterbitkan dan tidak dapat diubah");
    }

    if status == "diterbitkan" {
        let result = sqlx::query("INSERT INTO tu_nomor_surat (nomor, request_id) VALUES (?,?)")
            .bind(nomor.to_lowercase()).bind(&id).execute(&mut *tx).await;
        if let Err(err) = result {
            let duplicate = err.as_database_error().map(|db| db.is_unique_violation()).unwrap_or(false);
            error!("Gagal mencatat nomor surat: {}", err);
            let _ = tx.rollback().await;
            if duplicate {
                return error_json(actix_web::http::StatusCode::CONFLICT, "Nomor surat sudah pernah digunakan");
            }
            return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal mencatat nomor surat");
        }
    }

    let tanggal_terbit = if status == "diterbitkan" {
        chrono::Utc::now().format("%Y-%m-%d").to_string()
    } else {
        String::new()
    };

    surat[index].status = status.clone();
    surat[index].nomor = if status == "diterbitkan" { nomor } else { String::new() };
    surat[index].catatan = catatan;
    surat[index].tanggal_terbit = tanggal_terbit;
    let updated = match serde_json::to_string(&surat) {
        Ok(value) => value,
        Err(err) => {
            error!("Gagal menyimpan JSON surat: {}", err);
            let _ = tx.rollback().await;
            return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal memproses surat");
        }
    };

    if let Err(err) = sqlx::query("UPDATE tu_pengajuan SET surat=?, updated_at=CURRENT_TIMESTAMP WHERE id=?")
        .bind(updated).bind(&id).execute(&mut *tx).await {
        error!("Gagal memperbarui status surat: {}", err);
        let _ = tx.rollback().await;
        return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal memperbarui status surat");
    }

    if let Err(err) = tx.commit().await {
        error!("Gagal menyelesaikan transaksi penerbitan: {}", err);
        return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal menyimpan proses surat");
    }

    info!("Surat {} untuk pengajuan {} diproses oleh {}", jenis, id, staff.email);
    HttpResponse::Ok().json(serde_json::json!({ "ok": true }))
}


fn format_tanggal(value: &str) -> String {
    let months = ["Januari","Februari","Maret","April","Mei","Juni","Juli","Agustus","September","Oktober","November","Desember"];
    if let Some(date) = valid_date(value) {
        format!("{} {} {}", date.day(), months[(date.month() - 1) as usize], date.year())
    } else {
        "-".to_string()
    }
}

fn school_year(value: &str) -> String {
    if let Some(date) = valid_date(value) {
        let year = date.year();
        if date.month() >= 7 { format!("{}/{}", year, year + 1) }
        else { format!("{}/{}", year - 1, year) }
    } else {
        "-".to_string()
    }
}

fn wrap_pdf_text(value: &str, max_chars: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in value.split_whitespace() {
        let candidate = if current.is_empty() { word.to_string() } else { format!("{} {}", current, word) };
        if candidate.chars().count() > max_chars && !current.is_empty() {
            lines.push(current);
            current = word.to_string();
        } else {
            current = candidate;
        }
    }
    if !current.is_empty() { lines.push(current); }
    lines
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;')
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn make_letter_pdf_chrome(
    data: &serde_json::Value,
    surat: &Surat,
) -> Result<Vec<u8>, String> {
    use std::fs;
    use std::process::Command;

    let template_dir = "/opt/educonnect-tu/templates/html";

    let template_name = match surat.jenis.as_str() {
        "aktif" => "surat_aktif.html",
        "skhun" => "surat_skhun.html",
        _ => return Err("Jenis surat tidak valid".to_string()),
    };

    let template_path = format!("{}/{}", template_dir, template_name);

    let mut html = fs::read_to_string(&template_path)
        .map_err(|e| format!("Gagal membaca template: {}", e))?;

    let get = |key: &str| -> String {
        data.get(key)
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    let nama = escape_html(&get("nama"));
    let tempat = escape_html(&get("tempatLahir"));
    let tanggal_lahir = escape_html(&format_tanggal(&get("tanggalLahir")));
    let ttl = format!("{}, {}", tempat, tanggal_lahir);
    let nisn = escape_html(&get("nisn"));
    let kelas = escape_html(&get("kelas"));
    let nama_ortu = escape_html(&get("namaOrtu"));
    let alamat = escape_html(&get("alamat")).replace('\n', "<br>");
    let tahun_lulus = escape_html(&get("tahunLulus"));
    let tahun_pelajaran = escape_html(&school_year(&get("tanggalPengajuan")));
    let nomor = escape_html(&surat.nomor);
    let tanggal_terbit = escape_html(&format_tanggal(&surat.tanggal_terbit));

    let replacements = [
        ("{{nomor}}", nomor),
        ("{{nama}}", nama),
        ("{{ttl}}", ttl),
        ("{{nisn}}", nisn),
        ("{{kelas}}", kelas),
        ("{{nama_ortu}}", nama_ortu),
        ("{{alamat}}", alamat),
        ("{{tahun_lulus}}", tahun_lulus),
        ("{{tahun_pelajaran}}", tahun_pelajaran),
        ("{{tanggal_terbit}}", tanggal_terbit),
    ];

    for (placeholder, value) in replacements {
        html = html.replace(placeholder, &value);
    }

    let id = Uuid::new_v4().to_string();

    // HTML sementara berada di folder template agar logo relatif tetap ditemukan.
    let html_path = format!("{}/render-{}.html", template_dir, id);
    let pdf_path = format!(
        "{}/surat-{}.pdf",
        std::env::temp_dir().display(),
        id
    );

    fs::write(&html_path, html)
        .map_err(|e| format!("Gagal menulis HTML sementara: {}", e))?;

    let chrome = "/opt/chrome-headless/chrome-headless-shell-linux64/chrome-headless-shell";
    let file_url = format!("file://{}", html_path);

    let result = Command::new(chrome)
        .arg("--no-sandbox")
        .arg("--disable-gpu")
        .arg("--disable-dev-shm-usage")
        .arg("--no-zygote")
        .arg("--no-pdf-header-footer")
        .arg(format!("--print-to-pdf={}", pdf_path))
        .arg(file_url)
        .output();

    let _ = fs::remove_file(&html_path);

    let output = match result {
        Ok(output) => output,
        Err(e) => {
            let _ = fs::remove_file(&pdf_path);
            return Err(format!("Gagal menjalankan Chrome: {}", e));
        }
    };

    if !output.status.success() {
        let _ = fs::remove_file(&pdf_path);
        return Err(format!(
            "Chrome gagal membuat PDF: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    let pdf = fs::read(&pdf_path)
        .map_err(|e| format!("Gagal membaca hasil PDF: {}", e))?;

    let _ = fs::remove_file(&pdf_path);

    if pdf.is_empty() || !pdf.starts_with(b"%PDF-") {
        return Err("Hasil PDF tidak valid".to_string());
    }

    Ok(pdf)
}
#[get("/api/admin/pengajuan/{id}/surat/{jenis}/pdf")]
async fn download_letter_pdf(
    req: HttpRequest,
    state: web::Data<AppState>,
    path: web::Path<(String, String)>,
) -> HttpResponse {
    if let Err(response) = authorize(&req, &state).await {
        return response;
    }
    let (id, jenis) = path.into_inner();
    if jenis != "aktif" && jenis != "skhun" {
        return error_json(actix_web::http::StatusCode::BAD_REQUEST, "Jenis surat tidak valid");
    }
    let row = sqlx::query(
        "SELECT id,nama,tempat_lahir,DATE_FORMAT(tanggal_lahir,'%Y-%m-%d') AS tanggal_lahir,nisn,DATE_FORMAT(tanggal_pengajuan,'%Y-%m-%d') AS tanggal_pengajuan,kelas,nama_ortu,alamat,tahun_lulus,CAST(surat AS CHAR) AS surat_json FROM tu_pengajuan WHERE id=? LIMIT 1"
    ).bind(&id).fetch_optional(&state.db).await;
    let row = match row {
        Ok(Some(row)) => row,
        Ok(None) => return error_json(actix_web::http::StatusCode::NOT_FOUND, "Pengajuan tidak ditemukan"),
        Err(err) => {
            error!("Gagal mengambil data surat: {}", err);
            return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal mengambil data surat");
        }
    };
    let raw: String = row.get("surat_json");
    let surat: Vec<Surat> = match serde_json::from_str(&raw) {
        Ok(value) => value,
        Err(err) => {
            error!("Data surat tidak valid: {}", err);
            return error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Data surat tidak valid");
        }
    };
    let Some(item) = surat.into_iter().find(|s| s.jenis == jenis) else {
        return error_json(actix_web::http::StatusCode::NOT_FOUND, "Jenis surat tidak diajukan");
    };
    if item.status != "diterbitkan" {
        return error_json(actix_web::http::StatusCode::FORBIDDEN, "Surat belum diterbitkan");
    }
    let data = serde_json::json!({
        "id": row.get::<String,_>("id"),
        "nama": row.get::<String,_>("nama"),
        "tempatLahir": row.get::<String,_>("tempat_lahir"),
        "tanggalLahir": row.get::<String,_>("tanggal_lahir"),
        "nisn": row.get::<String,_>("nisn"),
        "tanggalPengajuan": row.get::<String,_>("tanggal_pengajuan"),
        "kelas": row.try_get::<Option<String>,_>("kelas").ok().flatten().unwrap_or_default(),
        "namaOrtu": row.try_get::<Option<String>,_>("nama_ortu").ok().flatten().unwrap_or_default(),
        "alamat": row.try_get::<Option<String>,_>("alamat").ok().flatten().unwrap_or_default(),
        "tahunLulus": row.try_get::<Option<String>,_>("tahun_lulus").ok().flatten().unwrap_or_default()
    });
    match make_letter_pdf_chrome(&data, &item) {
        Ok(bytes) => HttpResponse::Ok()
            .insert_header(("Content-Type", "application/pdf"))
            .insert_header(("Content-Disposition", format!("attachment; filename=\"surat-{}-{}.pdf\"", jenis, data["nisn"].as_str().unwrap_or("siswa"))))
            .body(bytes),
        Err(err) => {
            error!("Gagal membuat PDF: {}", err);
            error_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "Gagal membuat PDF surat")
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

    let state = web::Data::new(AppState { db, http: reqwest::Client::new() });
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
            .service(health).service(create_pengajuan).service(get_status).service(download_public_letter_pdf).service(admin_me).service(list_staff).service(add_staff).service(delete_staff).service(delete_pengajuan).service(list_pengajuan).service(process_letter).service(download_letter_pdf)
    }).bind(("127.0.0.1", port))?.run().await
}
