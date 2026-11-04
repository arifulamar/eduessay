
import express from 'express';
import cors from 'cors';
import crypto from 'node:crypto';
import PDFDocument from 'pdfkit';
import mysql from 'mysql2/promise';
import admin from 'firebase-admin';

if (!admin.apps.length) {
  admin.initializeApp();
}

const db = mysql.createPool({
  host: process.env.DB_HOST || '127.0.0.1',
  port: Number(process.env.DB_PORT || 3306),
  user: process.env.DB_USER || 'educonnect',
  password: process.env.DB_PASSWORD,
  database: process.env.DB_NAME || 'educonnect_tu',
  waitForConnections: true,
  connectionLimit: Number(process.env.DB_POOL_SIZE || 2),
  charset: 'utf8mb4',
  dateStrings: true
});

const app = express();
app.disable('x-powered-by');
app.use(express.json({ limit: '20kb' }));

const origins = (process.env.ALLOWED_ORIGINS || 'https://educonnect.my.id')
  .split(',')
  .map(x => x.trim())
  .filter(Boolean);

app.use(cors({
  origin: (origin, cb) => cb(null, !origin || origins.includes(origin)),
  methods: ['GET', 'POST', 'PATCH', 'DELETE'],
  allowedHeaders: ['Content-Type', 'Authorization']
}));

const types = ['aktif', 'skhun'];

const clean = (x, max = 120) =>
  typeof x === 'string' ? x.trim().slice(0, max) : '';

const validDate = x =>
  /^\d{4}-\d{2}-\d{2}$/.test(x) &&
  !Number.isNaN(Date.parse(x));

const months = [
  'Januari', 'Februari', 'Maret', 'April',
  'Mei', 'Juni', 'Juli', 'Agustus',
  'September', 'Oktober', 'November', 'Desember'
];

const tanggal = x => {
  if (!validDate(x)) return '-';
  const [y, m, d] = x.split('-').map(Number);
  return `${d} ${months[m - 1]} ${y}`;
};

const fail = (res, status, error) =>
  res.status(status).json({ error });

const SUPERADMIN_EMAILS = (
  process.env.SUPERADMIN_EMAILS || 'ariful.amar@gmail.com'
).toLowerCase().split(',').map(x => x.trim()).filter(Boolean);

const limiter = new Map();

function limit(req, res, next) {
  const now = Date.now();
  const key = req.ip || 'unknown';
  const hits = (limiter.get(key) || [])
    .filter(t => now - t < 3600000);

  if (hits.length >= 12) {
    return fail(res, 429, 'Terlalu banyak pengajuan. Coba kembali nanti.');
  }

  hits.push(now);
  limiter.set(key, hits);
  next();
}

async function requireAdmin(req, res, next) {
  try {
    const token = (req.headers.authorization || '')
      .match(/^Bearer (.+)$/)?.[1];

    if (!token) {
      return fail(res, 401, 'Login diperlukan');
    }

    const decoded = await admin.auth().verifyIdToken(token, true);

    if (!decoded.email_verified || !decoded.email) {
      return fail(res, 403, 'Email Google terverifikasi diperlukan');
    }

    const email = decoded.email.toLowerCase();
    const superadmin = SUPERADMIN_EMAILS.includes(email);

    const [rows] = await db.execute(
      'SELECT email FROM tu_admins WHERE email=? LIMIT 1',
      [email]
    );

    if (!superadmin && !rows.length) {
      return fail(res, 403, 'Email ini belum diizinkan sebagai staf TU');
    }

    req.staff = email;
    req.isSuperadmin = superadmin;
    next();

  } catch (e) {
    console.error('Auth error:', e.message);
    return fail(
      res,
      401,
      'Sesi tidak valid atau layanan autentikasi belum dikonfigurasi'
    );
  }
}

function requireSuperadmin(req, res, next) {
  if (!req.isSuperadmin) {
    return fail(res, 403, 'Khusus Superadmin');
  }
  next();
}

function parseRequest(row) {
  return {
    id: row.id,
    nama: row.nama,
    tempatLahir: row.tempat_lahir,
    tanggalLahir: row.tanggal_lahir,
    nisn: row.nisn,
    tanggalPengajuan: row.tanggal_pengajuan,
    kelas: row.kelas || '',
    namaOrtu: row.nama_ortu || '',
    alamat: row.alamat || '',
    tahunLulus: row.tahun_lulus || '',
    surat: typeof row.surat === 'string'
      ? JSON.parse(row.surat)
      : row.surat
  };
}

// HEALTH CHECK
app.get('/health', async (_req, res) => {
  try {
    await db.query('SELECT 1');
    res.json({ ok: true, database: 'connected' });
  } catch (e) {
    console.error(e);
    fail(res, 503, 'Database belum terhubung');
  }
});

// IDENTITAS ADMIN
app.get('/api/admin/me', requireAdmin, (req, res) => {
  res.json({
    email: req.staff,
    role: req.isSuperadmin ? 'superadmin' : 'staf'
  });
});

// DAFTAR STAF
app.get('/api/superadmin/staff', requireAdmin, requireSuperadmin, async (_req, res) => {
  try {
    const [rows] = await db.execute(
      'SELECT email, created_at AS addedAt FROM tu_admins ORDER BY email'
    );
    res.json(rows);
  } catch (e) {
    console.error(e);
    fail(res, 500, 'Gagal memuat akun staf');
  }
});

// TAMBAH STAF
app.post('/api/superadmin/staff', requireAdmin, requireSuperadmin, async (req, res) => {
  const email = clean(req.body?.email, 254).toLowerCase();

  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
    return fail(res, 400, 'Alamat email tidak valid');
  }

  if (SUPERADMIN_EMAILS.includes(email)) {
    return fail(res, 400, 'Email Superadmin tidak perlu ditambahkan sebagai staf');
  }

  try {
    await db.execute(
      `INSERT INTO tu_admins (email, added_by)
       VALUES (?,?)
       ON DUPLICATE KEY UPDATE added_by=VALUES(added_by)`,
      [email, req.staff]
    );

    res.status(201).json({ ok: true, email });

  } catch (e) {
    console.error(e);
    fail(res, 500, 'Gagal menambahkan staf');
  }
});

// HAPUS STAF
app.delete('/api/superadmin/staff/:email', requireAdmin, requireSuperadmin, async (req, res) => {
  const email = clean(decodeURIComponent(req.params.email), 254).toLowerCase();

  if (!email || SUPERADMIN_EMAILS.includes(email)) {
    return fail(res, 400, 'Tidak dapat menghapus akun Superadmin');
  }

  try {
    await db.execute(
      'DELETE FROM tu_admins WHERE email=?',
      [email]
    );

    res.json({ ok: true });

  } catch (e) {
    console.error(e);
    fail(res, 500, 'Gagal menghapus akses staf');
  }
});

// PENGAJUAN SURAT
app.post('/api/pengajuan', limit, async (req, res) => {
  try {
    const b = req.body || {};
    const jenis = Array.isArray(b.jenis)
      ? [...new Set(b.jenis)]
      : [];

    if (!jenis.length || jenis.length > 2 ||
        jenis.some(x => !types.includes(x))) {
      return fail(res, 400, 'Pilih jenis surat yang valid');
    }

    const nama = clean(b.nama);
    const tempatLahir = clean(b.tempatLahir, 100);
    const tanggalLahir = clean(b.tanggalLahir, 10);
    const nisn = clean(b.nisn, 10);
    const tanggalPengajuan = clean(b.tanggalPengajuan, 10);
    const kelas = clean(b.kelas, 40);
    const namaOrtu = clean(b.namaOrtu);
    const alamat = clean(b.alamat, 500);
    const tahunLulus = clean(b.tahunLulus, 9);

    if (!nama || !tempatLahir ||
        !/^\d{10}$/.test(nisn) ||
        !validDate(tanggalLahir) ||
        !validDate(tanggalPengajuan)) {
      return fail(res, 400, 'Lengkapi identitas dan NISN 10 digit');
    }

    if (jenis.includes('aktif') && !kelas) {
      return fail(res, 400, 'Kelas wajib diisi');
    }

    if (
      jenis.includes('skhun') &&
      (
        !namaOrtu ||
        !alamat ||
        !/^\d{4}\/\d{4}$/.test(tahunLulus) ||
        Number(tahunLulus.slice(5)) !== Number(tahunLulus.slice(0, 4)) + 1
      )
    ) {
      return fail(
        res,
        400,
        'Nama orang tua, alamat, dan tahun pelajaran lulus wajib diisi'
      );
    }

    if (
      Date.parse(tanggalLahir) > Date.now() ||
      Date.parse(tanggalPengajuan) > Date.now() + 86400000
    ) {
      return fail(res, 400, 'Tanggal tidak valid');
    }

    const id = crypto.randomUUID();
    const kode = crypto.randomBytes(16).toString('hex').toUpperCase();
    const kodeHash = crypto.createHash('sha256')
      .update(kode)
      .digest('hex');

    const surat = jenis.map(j => ({
      jenis: j,
      status: 'menunggu',
      nomor: '',
      catatan: '',
      tanggalTerbit: ''
    }));

    await db.execute(
      `INSERT INTO tu_pengajuan
       (id,nama,tempat_lahir,tanggal_lahir,nisn,tanggal_pengajuan,
        kelas,nama_ortu,alamat,tahun_lulus,kode_hash,surat)
       VALUES (?,?,?,?,?,?,?,?,?,?,?,?)`,
      [
        id, nama, tempatLahir, tanggalLahir, nisn,
        tanggalPengajuan, kelas, namaOrtu, alamat,
        tahunLulus, kodeHash, JSON.stringify(surat)
      ]
    );

    res.status(201).json({ kode });

  } catch (e) {
    console.error(e);
    fail(res, 500, 'Gagal menyimpan pengajuan');
  }
});

// CEK STATUS PENGAJUAN
app.get('/api/status/:kode', async (req, res) => {
  try {
    const kode = clean(req.params.kode, 64);

    if (!/^[A-F0-9]{32}$/.test(kode)) {
      return fail(res, 400, 'Kode tidak valid');
    }

    const hash = crypto.createHash('sha256')
      .update(kode)
      .digest('hex');

    const [rows] = await db.execute(
      'SELECT surat FROM tu_pengajuan WHERE kode_hash=? LIMIT 1',
      [hash]
    );

    if (!rows.length) {
      return fail(res, 404, 'Pengajuan tidak ditemukan');
    }

    const surat = typeof rows[0].surat === 'string'
      ? JSON.parse(rows[0].surat)
      : rows[0].surat;

    res.json({
      surat: surat.map(({ jenis, status, catatan }) => ({
        jenis,
        status,
        catatan
      }))
    });

  } catch (e) {
    console.error(e);
    fail(res, 500, 'Gagal memeriksa status');
  }
});

// DAFTAR PENGAJUAN ADMIN
app.get('/api/admin/pengajuan', requireAdmin, async (_req, res) => {
  try {
    const [rows] = await db.execute(
      'SELECT * FROM tu_pengajuan ORDER BY created_at DESC LIMIT 200'
    );

    res.json(rows.map(parseRequest));

  } catch (e) {
    console.error(e);
    fail(res, 500, 'Gagal memuat pengajuan');
  }
});

// PROSES SURAT
app.patch('/api/admin/pengajuan/:id/surat/:jenis', requireAdmin, async (req, res) => {
  const { id, jenis } = req.params;

  if (!types.includes(jenis)) {
    return fail(res, 400, 'Jenis surat tidak valid');
  }

  const status = clean(req.body?.status);
  const nomor = clean(req.body?.nomor, 100);
  const catatan = clean(req.body?.catatan, 500);

  if (!['perbaikan', 'ditolak', 'diterbitkan'].includes(status)) {
    return fail(res, 400, 'Status tidak valid');
  }

  if (status === 'diterbitkan' && !nomor) {
    return fail(res, 400, 'Nomor surat wajib diisi');
  }

  if (status === 'perbaikan' && !catatan) {
    return fail(res, 400, 'Tuliskan catatan perbaikan');
  }

  let conn;

  try {
    conn = await db.getConnection();
    await conn.beginTransaction();

    const [rows] = await conn.execute(
      'SELECT surat FROM tu_pengajuan WHERE id=? FOR UPDATE',
      [id]
    );

    if (!rows.length) {
      throw Error('Pengajuan tidak ditemukan');
    }

    const surat = typeof rows[0].surat === 'string'
      ? JSON.parse(rows[0].surat)
      : rows[0].surat;

    const idx = surat.findIndex(s => s.jenis === jenis);

    if (idx < 0) {
      throw Error('Jenis surat tidak diajukan');
    }

    if (surat[idx].status === 'diterbitkan') {
      throw Error('Surat sudah diterbitkan dan tidak dapat diubah');
    }

    if (status === 'diterbitkan') {
      await conn.execute(
        'INSERT INTO tu_nomor_surat (nomor, request_id) VALUES (?,?)',
        [nomor.toLowerCase(), id]
      );
    }

    surat[idx] = {
      ...surat[idx],
      status,
      nomor: status === 'diterbitkan' ? nomor : '',
      catatan,
      tanggalTerbit: status === 'diterbitkan'
        ? new Date().toLocaleDateString('en-CA', {
            timeZone: 'Asia/Jakarta'
          })
        : '',
      diprosesOleh: req.staff
    };

    await conn.execute(
      'UPDATE tu_pengajuan SET surat=?, updated_at=CURRENT_TIMESTAMP WHERE id=?',
      [JSON.stringify(surat), id]
    );

    await conn.commit();
    res.json({ ok: true });

  } catch (e) {
    if (conn) {
      try {
        await conn.rollback();
      } catch (rollbackError) {
        console.error('Rollback error:', rollbackError);
      }
    }

    fail(
      res,
      e.code === 'ER_DUP_ENTRY' ? 409 : 400,
      e.code === 'ER_DUP_ENTRY'
        ? 'Nomor surat sudah pernah digunakan'
        : e.message
    );

  } finally {
    if (conn) conn.release();
  }
});

// TAHUN PELAJARAN
function schoolYear(date) {
  const [y, m] = date.split('-').map(Number);
  return m >= 7 ? `${y}/${y + 1}` : `${y - 1}/${y}`;
}

// PEMBUATAN PDF
function makePdf(d, s) {
  const pdf = new PDFDocument({
    size: 'A4',
    margins: {
      top: 48,
      bottom: 50,
      left: 62,
      right: 62
    },
    info: {
      Title: 'Surat Keterangan SMA Negeri 1 Sabang'
    }
  });

  const W = pdf.page.width;

  pdf.font('Helvetica-Bold')
    .fontSize(13)
    .text('PEMERINTAH ACEH', { align: 'center' })
    .text('DINAS PENDIDIKAN', { align: 'center' })
    .text('SMA NEGERI 1 SABANG', { align: 'center' });

  pdf.font('Helvetica')
    .fontSize(9)
    .text('Jalan T. Nyak Arief, Gampong Ie Meulee, Sabang 23521', {
      align: 'center'
    })
    .text(
      'Telp. (0652) 21240 | www.sman1sabang.sch.id | sman1sabang@gmail.com',
      { align: 'center' }
    );

  pdf.moveDown(0.5);
  pdf.moveTo(62, pdf.y).lineTo(W - 62, pdf.y).lineWidth(2).stroke();
  pdf.moveDown(2);

  pdf.font('Times-Bold')
    .fontSize(14)
    .text('SURAT KETERANGAN', {
      align: 'center',
      underline: true
    });

  pdf.font('Times-Roman')
    .fontSize(12)
    .text('Nomor : ' + s.nomor, { align: 'center' });

  pdf.moveDown(3);

  pdf.text(
    'Yang bertanda tangan di bawah ini, Kepala SMA Negeri 1 Sabang, menerangkan bahwa:',
    { align: 'justify' }
  );

  pdf.moveDown(1);

  const rows = [
    ['Nama', d.nama],
    ['Tempat/Tanggal Lahir', d.tempatLahir + ', ' + tanggal(d.tanggalLahir)],
    ['NISN', d.nisn],
    ...(s.jenis === 'aktif'
      ? [['Kelas', d.kelas]]
      : [['Nama Orang Tua', d.namaOrtu], ['Alamat', d.alamat]])
  ];

  for (const [k, v] of rows) {
    const y = pdf.y;
    pdf.text(k, 80, y, { width: 150 });
    pdf.text(':', 235, y);
    pdf.text(v, 250, y, { width: 290 });
    pdf.y = Math.max(pdf.y, y + 22);
  }

  pdf.moveDown(1.5);

  if (s.jenis === 'aktif') {
    pdf.text(
      'Benar bahwa nama tersebut di atas adalah siswa SMA Negeri 1 Sabang Tahun Pelajaran ' +
      schoolYear(d.tanggalPengajuan) +
      ', dan saat ini masih duduk di kelas ' + d.kelas + '.',
      { align: 'justify', lineGap: 5 }
    );
  } else {
    pdf.text(
      'Benar bahwa nama tersebut di atas adalah lulusan SMA Negeri 1 Sabang Tahun Pelajaran ' +
      d.tahunLulus +
      ', yang pada tahun tersebut pemerintah hanya menerbitkan IJAZAH dan tidak menerbitkan Surat Keterangan Hasil Ujian Nasional (SKHUN).',
      { align: 'justify', lineGap: 5 }
    );
  }

  pdf.moveDown(1.5);

  pdf.text(
    'Demikian surat keterangan ini dibuat dengan sebenarnya untuk dipergunakan seperlunya.',
    { align: 'justify', lineGap: 5 }
  );

  pdf.moveDown(3);

  const y = pdf.y;

  pdf.text('Sabang, ' + tanggal(s.tanggalTerbit), 355, y, {
    width: 190
  });

  pdf.text('Kepala SMA Negeri 1 Sabang', 355, pdf.y, {
    width: 190
  });

  pdf.moveDown(4);

  pdf.text(
    process.env.KEPALA_NAMA || '[Nama Kepala Sekolah]',
    355,
    pdf.y,
    {
      width: 190,
      underline: true
    }
  );

  pdf.text(
    process.env.KEPALA_NIP
      ? 'NIP. ' + process.env.KEPALA_NIP
      : 'NIP. [Isi NIP]',
    355,
    pdf.y,
    {
      width: 190
    }
  );

  return pdf;
}

// DOWNLOAD PDF
app.get('/api/admin/pengajuan/:id/surat/:jenis/pdf', requireAdmin, async (req, res) => {
  try {
    if (!types.includes(req.params.jenis)) {
      return fail(res, 400, 'Jenis tidak valid');
    }

    const [rows] = await db.execute(
      'SELECT * FROM tu_pengajuan WHERE id=? LIMIT 1',
      [req.params.id]
    );

    if (!rows.length) {
      return fail(res, 404, 'Pengajuan tidak ditemukan');
    }

    const d = parseRequest(rows[0]);
    const s = d.surat.find(x => x.jenis === req.params.jenis);

    if (!s || s.status !== 'diterbitkan') {
      return fail(res, 403, 'Surat belum diterbitkan');
    }

    res.setHeader('Content-Type', 'application/pdf');
    res.setHeader(
      'Content-Disposition',
      `attachment; filename="surat-${s.jenis}-${d.nisn}.pdf"`
    );

    const pdf = makePdf(d, s);
    pdf.pipe(res);
    pdf.end();

  } catch (e) {
    console.error(e);

    if (!res.headersSent) {
      fail(res, 500, 'Gagal membuat PDF');
    } else {
      res.destroy(e);
    }
  }
});

// SERVER
const port = Number(process.env.PORT || 8080);

const server = app.listen(port, () => {
  console.log('Educonnect Tata Usaha API listening on ' + port);
});

async function shutdown() {
  server.close(async () => {
    await db.end();
    process.exit(0);
  });
}

process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);
