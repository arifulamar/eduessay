# Cloud Run deployment

Run from the repository root:

```bash
gcloud config set project eduessay-bd0ae
gcloud services enable run.googleapis.com cloudbuild.googleapis.com firestore.googleapis.com
gcloud run deploy educonnect-tu --source ./tatausaha-api --region asia-southeast2 --allow-unauthenticated --set-env-vars "ALLOWED_ORIGINS=https://educonnect.my.id,ADMIN_EMAILS=,KEPALA_NAMA=,KEPALA_NIP="
```

After deployment, copy the HTTPS service URL and replace `API_BASE` in `tatausaha/config.js` with that URL (no trailing slash). Redeploy/recommit the frontend file.

## Required service account
The Cloud Run runtime service account needs Firestore access (for example, Cloud Datastore User) in project `eduessay-bd0ae`. Do not upload service-account JSON keys to GitHub; use the attached runtime service account.

## Before opening to the public
1. Add staff Google email addresses to `ADMIN_EMAILS` in Cloud Run.
2. Set the verified principal's name and NIP with `KEPALA_NAMA` and `KEPALA_NIP`.
3. Confirm Google sign-in is enabled in Firebase Authentication and `educonnect.my.id` is an authorized domain.
4. Confirm Firestore is initialized in the same Firebase/GCP project.
5. Test a request, staff verification, unique letter number, and generated PDF.
6. Verify letter wording and signature authority with school administration before official use.

The API uses Firebase Admin SDK and therefore accesses Firestore using the Cloud Run service account. Do not grant public Firestore access to the `tu_pengajuan` or `tu_nomor_surat` collections.
