# Educonnect Tata Usaha — SMA Negeri 1 Sabang

Frontend layanan surat tetap di GitHub Pages, sedangkan backend mandiri dijalankan pada VPS.

## Frontend
- Publik: https://educonnect.my.id/tatausaha/
- Dashboard staf: https://educonnect.my.id/tatausaha/admin/

## Backend mandiri
- Kode backend: `tatausaha-api/`
- Runtime: Node.js + Express
- Database: MySQL 8.4
- Deployment: Docker Compose pada VPS Ubuntu
- Panduan: [tatausaha-api/DEPLOY.md](../tatausaha-api/DEPLOY.md)

## Status konfigurasi
`config.js` masih menunggu alamat API HTTPS VPS. Setelah VPS siap dan DNS `api.educonnect.my.id` aktif, isi `API_BASE` dengan `https://api.educonnect.my.id`.

Google Sign-In di frontend masih menggunakan Firebase Authentication untuk identitas. Namun, data pengajuan, hak akses staf, dan nomor surat dikelola backend Node.js dan MySQL; backend tidak lagi menggunakan Firestore.

Jangan commit file rahasia `.env` atau `secrets/firebase-service-account.json` ke GitHub.
