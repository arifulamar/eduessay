# Backend Mandiri Educonnect Tata Usaha

Backend Node.js + Express + MySQL untuk SMA Negeri 1 Sabang. Frontend tetap di GitHub Pages.

## Arsitektur
- Frontend: https://educonnect.my.id/tatausaha/
- API: nantinya https://api.educonnect.my.id
- Runtime: Node.js 22 pada VPS Ubuntu melalui Docker Compose
- Database: MySQL 8.4 dengan volume persisten
- Login staf: Firebase Authentication Google Sign-In yang sudah dipakai frontend. Backend memverifikasi Firebase ID token melalui Firebase Admin SDK.
- Data pengajuan, daftar staf, dan nomor surat disimpan di MySQL. Firestore tidak digunakan oleh API ini.

## Isi direktori
- server.js: REST API dan generator PDF.
- schema.sql: skema tabel MySQL.
- Dockerfile, docker-compose.yml: deployment container.
- .env.example: contoh konfigurasi; salin menjadi .env di VPS dan isi password kuat.
- DEPLOY.md: petunjuk deployment.

## Endpoint
- GET /health
- POST /api/pengajuan
- GET /api/status/:kode
- GET /api/admin/me
- GET /api/admin/pengajuan
- PATCH /api/admin/pengajuan/:id/surat/:jenis
- GET /api/admin/pengajuan/:id/surat/:jenis/pdf
- GET/POST/DELETE /api/superadmin/staff

## Catatan autentikasi
Untuk mempertahankan tombol Google yang sudah ada, tahap pertama tetap menggunakan Firebase Authentication hanya untuk identitas/login. Hak akses staf dan semua data aplikasi berada di MySQL. VPS harus diberi kredensial Firebase Admin melalui file JSON di luar GitHub. Jangan pernah mengunggah file service-account ke repository.

## Keamanan
- Simpan .env dan secrets di VPS saja.
- Buka hanya port SSH dan HTTPS pada firewall; port API hanya bind ke localhost.
- Letakkan Nginx + HTTPS di depan API.
- Backup database secara rutin dan uji pemulihan.
- Batasi akses MySQL hanya dari jaringan internal Docker.
- Ganti password contoh sebelum menjalankan layanan.
