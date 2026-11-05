# Educonnect Tata Usaha — SMA Negeri 1 Sabang

Frontend layanan surat tetap di GitHub Pages. Backend mandiri dikembangkan menggunakan Rust + Actix Web pada VPS.

## Frontend
- Publik: https://educonnect.my.id/tatausaha/
- Dashboard staf: https://educonnect.my.id/tatausaha/admin/

## Backend Rust
- Kode backend: `tatausaha-api-rust/`
- Runtime: Rust + Actix Web
- Database: MySQL, database `educonnect_tu`
- Deployment: VPS Ubuntu dengan systemd dan Nginx
- Panduan teknis: `tatausaha-api-rust/README.md`

## Status migrasi
Backend Node.js lama telah dihapus dari repository. Backend Rust sedang dibangun bertahap. Saat ini tahap awal menyediakan server Actix Web dan pemeriksaan koneksi MySQL melalui `GET /health`.

**Penting:** Jangan mengubah `API_BASE` di `tatausaha/config.js` ke alamat produksi sampai seluruh endpoint Rust (pengajuan, status, Firebase auth, administrasi staf, proses surat, dan PDF) selesai dan diuji.

Google Sign-In pada frontend tetap menggunakan Firebase Authentication. Backend Rust akan memverifikasi Firebase ID Token dan menggunakan MySQL untuk data pengajuan, hak akses staf, serta nomor surat.

Jangan commit file rahasia `.env` atau Firebase service-account JSON ke GitHub.
