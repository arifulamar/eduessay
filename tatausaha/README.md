# Educonnect Tata Usaha — SMA Negeri 1 Sabang

Aplikasi layanan surat keterangan berbasis GitHub Pages + Firebase + Cloud Run.

## Frontend
- Publik: https://educonnect.my.id/tatausaha/
- Dashboard staf: https://educonnect.my.id/tatausaha/admin/

## Konfigurasi
Edit `config.js` setelah backend Cloud Run tersedia. Isi `API_BASE` dengan URL Cloud Run (tanpa garis miring akhir). Firebase Web config memakai project `eduessay-bd0ae`.

## Backend
Sumber backend ada di `tatausaha-api/`. Deploy dari folder tersebut ke Cloud Run:
```bash
gcloud run deploy educonnect-tu --source ./tatausaha-api --region asia-southeast2 --allow-unauthenticated --set-env-vars "ALLOWED_ORIGINS=https://educonnect.my.id,ADMIN_EMAILS="
```
Tambahkan email staf TU pada `ADMIN_EMAILS` setelah diterima. Jangan memasukkan service-account key ke repository. Gunakan service account Cloud Run dengan akses Firestore.

## Status
Frontend dan sumber backend telah ditempatkan pada repository GitHub Pages. Backend perlu dideploy dan URL Cloud Run dimasukkan ke `tatausaha/config.js` sebelum layanan dapat menerima pengajuan.