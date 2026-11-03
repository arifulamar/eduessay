# Cloud Run deployment

Run from the repository root:

```bash
gcloud config set project eduessay-bd0ae
gcloud services enable run.googleapis.com cloudbuild.googleapis.com firestore.googleapis.com
gcloud run deploy educonnect-tu --source ./tatausaha-api --region asia-southeast2 --allow-unauthenticated --set-env-vars "ALLOWED_ORIGINS=https://educonnect.my.id,SUPERADMIN_EMAILS=ariful.amar@gmail.com,ADMIN_EMAILS=,KEPALA_NAMA=,KEPALA_NIP="
```

After deployment, copy the HTTPS service URL and replace `API_BASE` in `tatausaha/config.js` with that URL (no trailing slash). Redeploy/recommit the frontend file.

## Required service account
The Cloud Run runtime service account needs Firestore access (for example, Cloud Datastore User) in project `eduessay-bd0ae`. Do not upload service-account JSON keys to GitHub; use the attached runtime service account.

## Before opening to the public
1. Superadmin utama: `ariful.amar@gmail.com` (configured by `SUPERADMIN_EMAILS`). This account can add/remove TU staff from the admin dashboard.
2. Sign in to `/tatausaha/admin/` with the Superadmin Google account and add staff emails in **Manajemen akun staf TU**. Staff access is stored in the private Firestore collection `tu_admins`.
3. Do not add the Superadmin email to the staff list; it has permanent elevated access through the server allowlist.
4. Set the verified principal's name and NIP with `KEPALA_NAMA` and `KEPALA_NIP`.
5. Confirm Google sign-in is enabled in Firebase Authentication and `educonnect.my.id` is an authorized domain.
6. Confirm Firestore is initialized in the same Firebase/GCP project.
7. Test a request, staff verification, unique letter number, and generated PDF.
8. Verify letter wording and signature authority with school administration before official use.

The API uses Firebase Admin SDK and therefore accesses Firestore using the Cloud Run service account. Do not grant public Firestore access to the `tu_pengajuan` or `tu_nomor_surat` collections.


## Superadmin security
- The backend grants the superadmin role only to verified Google ID tokens whose email matches `SUPERADMIN_EMAILS`.
- Superadmin-only routes manage staff records. Ordinary staff cannot list, add, or remove staff accounts.
- Never put admin email authorization only in frontend JavaScript; backend authorization is authoritative.
