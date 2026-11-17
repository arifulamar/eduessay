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
