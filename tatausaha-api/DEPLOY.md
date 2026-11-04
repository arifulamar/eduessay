# Deployment backend Tata Usaha di VPS Ubuntu

Panduan ini untuk VPS Ubuntu 22.04/24.04 dengan akses sudo. Backend menggunakan Node.js 22, Docker Compose, MySQL 8.4, dan Nginx. Jangan jalankan di GitHub Pages; Pages hanya menyajikan frontend statis.

## 1. DNS dan firewall
Buat DNS A record `api.educonnect.my.id` menuju IPv4 VPS. Di firewall/provider, izinkan TCP 22 (SSH), 80 dan 443. Jangan buka port 3306 atau 8080 ke internet.

## 2. Pasang Docker
Di VPS:
```bash
sudo apt update
sudo apt install -y ca-certificates curl git nginx
sudo install -m 0755 -d /etc/apt/keyrings
sudo curl -fsSL https://download.docker.com/linux/ubuntu/gpg -o /etc/apt/keyrings/docker.asc
sudo chmod a+r /etc/apt/keyrings/docker.asc
echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/docker.asc] https://download.docker.com/linux/ubuntu $(. /etc/os-release && echo "$VERSION_CODENAME") stable" | sudo tee /etc/apt/sources.list.d/docker.list > /dev/null
sudo apt update
sudo apt install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin
sudo systemctl enable --now docker
```

## 3. Ambil kode dari GitHub
```bash
sudo mkdir -p /opt/educonnect
sudo chown "$USER":"$USER" /opt/educonnect
git clone https://github.com/Arifulamar/eduessay.git /opt/educonnect/repo
cd /opt/educonnect/repo/tatausaha-api
```
Untuk repository private, gunakan autentikasi GitHub yang aman (SSH deploy key atau token dengan akses minimal); jangan tempel token di perintah yang tersimpan di shell history.

## 4. Siapkan konfigurasi rahasia
```bash
cp .env.example .env
mkdir -p secrets
chmod 700 secrets
nano .env
```
Ganti `DB_PASSWORD` dan `MYSQL_ROOT_PASSWORD` dengan dua password acak yang berbeda. Isi `KEPALA_NAMA` dan `KEPALA_NIP` jika sudah ditetapkan. Pertahankan `SUPERADMIN_EMAILS=ariful.amar@gmail.com`.
Buat Firebase service account JSON dari project Firebase `eduessay-bd0ae` melalui Google Cloud IAM > Service Accounts. Beri izin minimum untuk memverifikasi Firebase Authentication (Firebase Authentication Admin/Service Account Token Creator sesuai kebutuhan SDK), unduh JSON secara aman, lalu simpan sebagai `secrets/firebase-service-account.json`. File ini tidak boleh masuk GitHub.
```bash
chmod 600 .env secrets/firebase-service-account.json
```

## 5. Jalankan backend dan database
```bash
docker compose up -d --build
docker compose ps
docker compose logs -f api
```
Cek API dari VPS:
```bash
curl http://127.0.0.1:8080/health
```
Target: `{"ok":true,"database":"connected"}`.

Catatan: MySQL menjalankan `schema.sql` otomatis hanya saat volume database pertama kali dibuat. Jangan menghapus volume `mysql_data` untuk memperbaiki aplikasi karena itu menghapus data.

## 6. Pasang Nginx reverse proxy
Buat `/etc/nginx/sites-available/educonnect-tu`:
```nginx
server {
    listen 80;
    server_name api.educonnect.my.id;
    client_max_body_size 25k;
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_http_version 1.1;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```
Aktifkan:
```bash
sudo ln -s /etc/nginx/sites-available/educonnect-tu /etc/nginx/sites-enabled/educonnect-tu
sudo nginx -t
sudo systemctl reload nginx
sudo apt install -y certbot python3-certbot-nginx
sudo certbot --nginx -d api.educonnect.my.id
```
Tes HTTPS:
```bash
curl https://api.educonnect.my.id/health
```

## 7. Hubungkan frontend
Setelah HTTPS health check berhasil, ubah `tatausaha/config.js` di GitHub:
```js
export const API_BASE = 'https://api.educonnect.my.id';
```
Commit perubahan. Pastikan CORS `ALLOWED_ORIGINS=https://educonnect.my.id`.

## 8. Firebase Auth domain
Di Firebase Console > Authentication > Settings > Authorized domains, pastikan `educonnect.my.id` ada. Google Sign-In tetap digunakan untuk login; API memeriksa token dan mengecek peran dari tabel MySQL.

## 9. Backup
Contoh backup database:
```bash
docker compose exec -T db sh -c 'mysqldump -u root -p"$MYSQL_ROOT_PASSWORD" educonnect_tu' > backup-$(date +%F).sql
```
Simpan backup terenkripsi di lokasi terpisah dari VPS dan uji pemulihan secara berkala.

## 10. Pembaruan versi
```bash
cd /opt/educonnect/repo
git pull origin main
cd tatausaha-api
docker compose up -d --build
```

## Penting sebelum produksi
- Backend versi MySQL ini belum otomatis memigrasikan data lama yang mungkin ada di Firestore. Jika sudah ada pengajuan nyata, ekspor dan migrasikan sebelum berpindah.
- File Firebase service account adalah rahasia. Jangan commit, jangan kirim melalui chat, dan batasi izin file.
- Atur firewall provider dan update OS secara rutin.
- Lakukan uji pengajuan, status, verifikasi staf, nomor surat duplikat, PDF, backup, dan restore sebelum diumumkan ke publik.
