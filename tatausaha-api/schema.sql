CREATE DATABASE IF NOT EXISTS educonnect_tu CHARACTER SET utf8mb4 COLLATE utf8mb4_unicode_ci;
USE educonnect_tu;

CREATE TABLE IF NOT EXISTS tu_pengajuan (
  id CHAR(36) PRIMARY KEY,
  nama VARCHAR(120) NOT NULL,
  tempat_lahir VARCHAR(100) NOT NULL,
  tanggal_lahir DATE NOT NULL,
  nisn CHAR(10) NOT NULL,
  tanggal_pengajuan DATE NOT NULL,
  kelas VARCHAR(40) NULL,
  nama_ortu VARCHAR(120) NULL,
  alamat VARCHAR(500) NULL,
  tahun_lulus VARCHAR(9) NULL,
  kode_hash CHAR(64) NOT NULL UNIQUE,
  kode_pengajuan CHAR(32) NULL,
  surat JSON NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  updated_at TIMESTAMP NULL DEFAULT NULL ON UPDATE CURRENT_TIMESTAMP,
  INDEX idx_pengajuan_created (created_at),
  INDEX idx_pengajuan_nisn (nisn)
) ENGINE=InnoDB;

CREATE TABLE IF NOT EXISTS tu_admins (
  email VARCHAR(254) PRIMARY KEY,
  added_by VARCHAR(254) NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP
) ENGINE=InnoDB;

CREATE TABLE IF NOT EXISTS tu_nomor_surat (
  id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT PRIMARY KEY,
  nomor VARCHAR(100) NOT NULL UNIQUE,
  request_id CHAR(36) NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
  CONSTRAINT fk_nomor_request FOREIGN KEY (request_id) REFERENCES tu_pengajuan(id)
) ENGINE=InnoDB;
