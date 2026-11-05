# Educonnect Tata Usaha API — Rust

Backend Tata Usaha untuk SMA Negeri 1 Sabang. Backend lama Node.js sudah dihapus; implementasi baru menggunakan Rust + Actix Web dan MySQL.

## Struktur
- `Cargo.toml`: dependensi Rust.
- `src/main.rs`: server Actix Web, koneksi MySQL, health check, pengajuan, dan pelacakan status.
- Database tetap menggunakan skema `../tatausaha-api/schema.sql`.

## Endpoint yang sudah tersedia
- `GET /health` — memeriksa koneksi MySQL.
- `POST /api/pengajuan` — menerima pengajuan surat aktif dan/atau SKHUN.
- `GET /api/status/{kode}` — memeriksa status menggunakan kode pelacakan.

Respons dan nama field dipertahankan agar cocok dengan frontend Tata Usaha yang sudah ada.

## Konfigurasi
Salin `.env.example` menjadi `.env` pada mesin server, lalu isi variabel berikut:
- `PORT=8081`
- `DB_HOST=127.0.0.1`
- `DB_PORT=3306`
- `DB_USER=educonnect`
- `DB_PASSWORD=` (isi hanya di server, jangan commit)
- `DB_NAME=educonnect_tu`
- `DB_POOL_SIZE=2`
- `ALLOWED_ORIGINS=https://educonnect.my.id`
- `FIREBASE_PROJECT_ID=eduessay-bd0ae`
- `SUPERADMIN_EMAILS=ariful.amar@gmail.com`
- `KEPALA_NAMA=`
- `KEPALA_NIP=`

## Build dan pemeriksaan
Dari root repository:
```bash
cargo check --manifest-path tatausaha-api-rust/Cargo.toml
cargo build --release --manifest-path tatausaha-api-rust/Cargo.toml
```

GitHub Actions menjalankan `cargo check` saat ada perubahan backend Rust.

## Tahap migrasi
Implementasi masih bertahap. Endpoint admin, verifikasi Firebase ID token, proses penerbitan surat, dan pembuatan PDF belum dipindahkan. Jangan arahkan `tatausaha/config.js` ke API ini sebelum semua endpoint tersebut selesai diuji.
