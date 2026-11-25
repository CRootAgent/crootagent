# 🌴 LONTAR CLI 🦀🤖

> **LONTAR CLI** adalah asisten pengembang cerdas (*Agentic Coding & Document Intelligence Assistant*) berbasis CLI yang ultra-ringan, berkinerja tinggi, dan hemat biaya (zero-cost first), dibangun dengan bahasa pemrograman **Rust**.

Lontar memadukan kapabilitas **NotebookLM** (analisis dokumen, panduan belajar, ekstraksi konteks), **OpenCode / AWS Kiro** (antarmuka terminal status box, interaksi interaktif, diff preview warna, prompt `@file`), dan **Universal Zero-Cost LLM Router** yang siap pakai dengan model-model AI gratis (Groq Llama 3.3, Google Gemini Flash, DeepSeek V3/R1, OpenRouter `:free`, GitHub Models, SambaNova, Mistral NeMo, dan Ollama lokal).

---

## 🎨 Identitas & Filosofi Nama

Nama **Lontar** berakar dari daun lontar (*Borassus flabellifer*), media naskah kuno Nusantara dan Bali yang digunakan selama berabad-abad untuk mencatat, melestarikan, dan menyebarkan ilmu pengetahuan, sastra, hukum, dan pengobatan tradisional.

Dalam era komputasi modern, **LONTAR CLI** melanjutkan tradisi tersebut sebagai instrumen digital yang membaca, menggrounding, menyintesis dokumen pengetahuan (*knowledge preservation*), dan mengeksekusi instruksi rekayasa perangkat lunak secara langsung di terminal Anda.

Logo Lontar dirancang secara geometris presisi menggunakan skill `/logo-design`, memadukan bilah daun lontar tradisional, inisial 'L', prompt panah CLI `>_`, dan titik jangkar pengetahuan (*grounding node*).

---

## ✨ Fitur-Fitur Utama

### 1. 📚 NotebookLM-in-Terminal (Document Grounding & Synthesis)
- **Multi-Format Ingestion:** Ekstraksi dan chunking otomatis dokumen `.md`, `.txt`, `.pdf`, serta berbagai berkas source code (`.rs`, `.py`, `.js`, `.ts`, `.json`, `.toml`).
- **Semantic Overlap Chunking:** Memecah dokumen menjadi potongan-potongan terstruktur (800 kata dengan overlap 100 kata) disertai tag sitasi sumber `[Source: ..., Chunk: #]`.
- **Sintesis Artefak Akademik & Proyek:**
  - `lontar study-guide "<topik>"`: Menghasilkan dokumen panduan belajar komprehensif (`STUDY_GUIDE.md`) lengkap dengan ikhtisar, konsep kunci, glosarium, dan kuis pemahaman.
  - `lontar prd "<ide>"`: Menyintesis dokumen kebutuhan produk (`PRD.md`) standar industri software engineering.
  - `lontar tasklist "<prd>"`: Menyusun daftar tugas teknis berurutan (`TASKLIST.md`) dengan checklist dan kriteria verifikasi.
- **Grounding Dinamis:** Konteks dokumen yang relevan otomatis diinjeksikan saat berdialog di sesi chat.

### 2. ⚡ Universal Zero-Cost LLM Router & 429 Failover
Lontar CLI dirancang agar siapa pun dapat langsung menggunakannya tanpa kendala biaya API:
- **Groq Cloud:** Llama 3.3 70B Versatile (kecepatan generasi hingga ~280 tokens/detik).
- **Google Gemini:** Gemini 1.5 Flash & Gemini 2.0 Flash (kuota gratis Google AI Studio).
- **OpenRouter Free Tier:** Model komunitas tanpa biaya (`meta-llama/llama-3.3-70b-instruct:free`, `deepseek/deepseek-r1:free`).
- **DeepSeek:** DeepSeek V3 dan DeepSeek R1 Reasoner.
- **GitHub Models:** Akses gratis ke GPT-4o-mini dan Phi-3.5 via GitHub Personal Access Token (PAT).
- **SambaNova Cloud:** Llama 3.3 70B dengan akselerator hardware RDU.
- **Mistral AI:** Mistral NeMo dan Codestral.
- **Ollama Local Offline:** 100% lokal tanpa koneksi internet (`http://localhost:11434/v1`).
- **Intelligent 429 Failover:** Jika salah satu provider mengalami *Rate Limit Exceeded* (HTTP 429), Lontar secara otomatis mengalihkan permintaan ke provider gratis berikutnya dalam rantai prioritas tanpa memutus alur kerja Anda.

### 3. 🖥️ Antarmuka Terminal Terinspirasi OpenCode & Kiro
- **Status Box Berwarna:** Kotak status informatif menampilkan nama sesi aktif, model & provider yang berjalan, jumlah dokumen yang tergrounding, dan kesiapan failover.
- **Perintah Baris Perintah (*Slash Commands*):**
  - `/model`: Melihat dan mengganti model AI secara dinamis.
  - `/context`: Menampilkan dokumen tergrounding dalam sesi.
  - `/context add <file>`: Menambahkan dokumen ke memori sesi aktif.
  - `/test`: Menjalankan suite pengujian unit dan mencatat hasilnya ke database.
  - `/clear`: Membersihkan layar terminal.
  - `/help`: Menampilkan ringkasan bantuan.
  - `/exit`: Menyimpan sesi dan keluar.
- **Sintaks `@file` Dinamis:** Cukup ketik `@nama_file` di prompt untuk menyertakan konten file secara instan.
- **Human-in-the-Loop (HITL) Diff Preview:** Setiap perubahan berkas (`write_file`, `edit_file`) menampilkan visualisasi *unified diff* berwarna (merah untuk penghapusan, hijau untuk penambahan) dan meminta konfirmasi `[Y/n]` sebelum mengeksekusi perubahan.

### 4. 🗄️ Database SQLite Lokal (CRUD Terstruktur)
Memenuhi standar akademik Proyek 2 D4 Teknik Informatika ULBI:
- Tabel `sessions`: Riwayat sesi, nama, model, dan total token.
- Tabel `documents`: Berkas ter-ingest, ukuran, tipe, dan jumlah chunk.
- Tabel `document_chunks`: Potongan teks kontekstual beserta estimasi token.
- Tabel `providers`: Registri provider AI, endpoint URL, dan status gratis.
- Tabel `test_logs`: Riwayat eksekusi pengujian (*test runner*) dan durasi eksekusi.

### 5. 🧪 Kualitas Kode & Pengujian (Code Coverage)
- **22 Unit & Integration Tests:** Meliputi pengujian konfigurasi, database CRUD, parser dokumen, LLM router failover, visual diff viewer, dan perkakas developer.
- **100% Test Pass Rate:** Lulus verifikasi `cargo test`.
- **Dukungan `cargo-tarpaulin`:** Siap untuk audit code coverage $\ge 80\%$.

---

## 📦 Panduan Instalasi Cepat

### 1. Menggunakan Script Otomatis (Rekomendasi)

**macOS & Linux:**
```bash
curl -fsSL https://raw.githubusercontent.com/CRootAgent/crootagent/main/install.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://raw.githubusercontent.com/CRootAgent/crootagent/main/install.ps1 | iex
```

### 2. Menggunakan Cargo (Rust Toolchain)
```bash
cargo install --path .
```

### 3. Build dari Sumber (Source Code)
```bash
git clone https://github.com/CRootAgent/crootagent.git
cd crootagent
cargo build --release
# Biner siap di: target/release/lontar
```

---

## 🚀 Panduan Pemakaian

### 1. Menyetel API Key Gratis

Lontar dapat membaca key dari *Environment Variable* atau konfigurasi lokal:

```bash
# Contoh menyetel Groq Cloud (Gratis)
lontar config set groq_api_key "gsk_..."

# Contoh menyetel Google Gemini (Gratis)
lontar config set gemini_api_key "AIzaSy..."

# Contoh menyetel OpenRouter Free
lontar config set openrouter_api_key "sk-or-v1-..."

# Menyetel provider default yang digunakan
lontar config set default_provider groq
```

Melihat status konfigurasi dan model:
```bash
lontar config show
lontar models
```

### 2. Mode Interaktif (Chat REPL)

Jalankan perintah `lontar` tanpa argumen:
```bash
lontar
```

Tampilan antarmuka:
```text
 ╔═════════════════════════════════════════════════════════════════════╗
 ║   🌴 LONTAR CLI - Zero-Cost AI Agent & Document Intelligence        ║
 ║   Kiro & OpenCode Terminal UX │ NotebookLM Grounding │ 100% Rust    ║
 ╚═════════════════════════════════════════════════════════════════════╝
╭─────────────────────────────────────────────────────────────────────────╮
│ Sesi   : Sesi-Eksplorasi (62956444)                                     │
│ Model  : llama-3.3-70b-versatile [groq]                                 │
│ Konteks: 2 dokumen tergrounding │ Failover 8 Free AI Ready              │
╰─────────────────────────────────────────────────────────────────────────╯
lontar> Tolong analisa arsitektur sistem pada @README.md
```

### 3. NotebookLM Document Grounding & Sintesis

**Menambahkan dokumen referensi:**
```bash
lontar context add pedoman_magang.pdf arsitektur_db.md
lontar context list
```

**Menyintesis Panduan Belajar (*Study Guide*):**
```bash
lontar study-guide "Prinsip Dasar Konkurensi Rust" --file panduan.md --output STUDY_GUIDE.md
```

**Menyintesis PRD & Tasklist:**
```bash
lontar prd "Sistem Notifikasi Realtime dengan WebSocket" --output PRD.md
lontar tasklist PRD.md --output TASKLIST.md
```

### 4. Inspeksi Database SQLite
```bash
lontar db stats
lontar db providers
lontar db logs
```

---

## 🧪 Menjalankan Pengujian (Unit & Integration Tests)

Untuk memverifikasi kehandalan seluruh komponen sistem:
```bash
cargo test
```

Untuk mengukur persentase cakupan kode (*code coverage*):
```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html --output-dir target/coverage
```

---

## 🏛️ Informasi Akademik Proyek 2

- **Judul Proyek:** Rancang Bangun LONTAR CLI: Multi-LLM Zero-Cost Agent dengan Grounding Dokumen NotebookLM dan Antarmuka Terminal Modular
- **Institusi:** Program Studi D4 Teknik Informatika, Universitas Logistik dan Bisnis Internasional (ULBI)
- **Dosen Pembimbing:** Rolly Maulana Awangga, S.T., M.T., CAIP.
- **Pengembang:** Ahmad Zakky Nurfauzan (NPM: 714220031)

---

## 📄 Lisensi

Proyek ini didistribusikan di bawah lisensi **GNU General Public License v3.0 (GPL-3.0)**.
