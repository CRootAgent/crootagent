# CRootAgent 🦀🤖

> **CRootAgent** adalah asisten pengembang cerdas (*agentic coding assistant*) berbasis CLI yang ultra-ringan, berkinerja tinggi, dan hemat sumber daya, dibangun dengan bahasa pemrograman **Rust**.

CRootAgent dirancang sebagai alternatif berkecepatan tinggi tanpa beban runtime (tanpa Node.js runtime overhead, tanpa Python interpreter) dengan kapabilitas setara **Claude Code**, **OpenAI Codex CLI**, **Google Antigravity CLI**, dan **AWS Kiro**.

---

## ✨ Fitur Utama

- **🚀 Native & Ultra-Lightweight (Rust):**
  - Binary tunggal mandiri (*single standalone binary*) berukuran kecil.
  - Penggunaan memori dasar hanya **~8–15 MB RAM** (dibandingkan agent berbasis Node/Python yang membutuhkan 120–250+ MB).
  - Waktu startup kilat (< 20 ms).

- **🌐 Multi-API & Multi-Provider Support:**
  - **OpenAI:** GPT-4o, GPT-4o-mini, o1, o3-mini.
  - **Anthropic Claude:** Claude 3.5 Sonnet, Claude 3.5 Haiku, Claude 3 Opus.
  - **Google Gemini:** Gemini 1.5 Pro, Gemini 1.5 Flash.
  - **Groq Cloud:** Llama 3.3 70B Versatile, Llama 3.1 8B Instant (latensi kilat).
  - **Ollama (Lokal):** Llama 3.2, Qwen 2.5 Coder, DeepSeek Coder secara offline.

- **🗂️ Multi-Session Management:**
  - Simpan dan lanjutkan riwayat percakapan secara terpisah.
  - Dukungan perpindahan provider & model dinamis di tengah sesi (`/provider`, `/model`).
  - Ekspor seluruh sesi ke dokumen Markdown (`/export`).

- **🔧 Developer Tool Execution (ReAct Loop):**
  - `read_file`: Membaca isi file dengan nomor baris dan rentang baris spesifik.
  - `write_file`: Menulis file baru atau menimpa file dengan auto-create direktori.
  - `edit_file`: *Surgical exact search & replace* tanpa merusak kode sekitar.
  - `run_command`: Mengeksekusi perintah shell lokal dengan timeout dan penangkapan stdout/stderr.
  - `list_dir`: Navigasi direktori dan penelusuran hierarki proyek.

- **🛡️ Human-in-the-Loop (HITL) Safety:**
  - Mematuhi pedoman sistem agentic akademik: tindakan destruktif (eksekusi terminal atau perubahan file) meminta konfirmasi pengguna `[y/N]` secara transparan, kecuali flag `--yes` / `-y` diberikan.
  - **Explainable AI (xAI):** Setiap respon menyertakan atribusi model nyata (`provider:model`) dan pencatatan argumen perkakas.

- **⚙️ Multi-OS Automated Release CI/CD:**
  - GitHub Actions matrix build otomatis untuk Linux (x86_64), macOS (Apple Silicon arm64 & Intel x86_64), dan Windows (x86_64).
  - Auto-tagging dan pembuatan GitHub Releases beserta checksum SHA256.

---

## 📦 Panduan Instalasi Cepat

### 1. Menggunakan Script Otomatis (Rekomendasi)

**Linux & macOS:**
```bash
curl -fsSL https://crootagent.github.io/install.sh | bash
```

**Windows (PowerShell):**
```powershell
irm https://crootagent.github.io/install.ps1 | iex
```

### 2. Menggunakan Cargo (Rust Toolchain)
```bash
cargo install --git https://github.com/CRootAgent/crootagent.git
```

### 3. Build dari Sumber (Source Code)
```bash
git clone https://github.com/CRootAgent/crootagent.git
cd crootagent
cargo build --release
# Binary berada di target/release/crootagent
```

---

## 🚀 Panduan Pemakaian

### 1. Konfigurasi API Key

CRootAgent dapat membaca API Key dari *environment variables* atau file konfigurasi lokal `~/.config/crootagent/config.toml`:

```bash
# Melalui Environment Variables
export GEMINI_API_KEY="AIzaSy..."
export GROQ_API_KEY="gsk_..."
export OPENAI_API_KEY="sk-..."
export ANTHROPIC_API_KEY="sk-ant-..."

# Atau melalui CLI config command
crootagent config set default_provider groq
crootagent config set groq_api_key "gsk_..."
crootagent config set gemini_api_key "AIzaSy..."
```

Melihat konfigurasi aktif:
```bash
crootagent config
```

### 2. Mode Interaktif (Chat REPL)

Jalankan perintah tanpa argumen untuk masuk ke sesi interaktif:
```bash
crootagent
```

Perintah dalam REPL:
- `/help` — Menampilkan panduan perintah.
- `/provider <nama>` — Mengganti provider (`openai`, `anthropic`, `gemini`, `groq`, `ollama`).
- `/model <nama>` — Mengganti model aktif.
- `/session <id/nama>` — Berpindah atau membuat sesi baru.
- `/export` — Mengekspor sesi aktif ke file Markdown.
- `/clear` — Membersihkan riwayat percakapan sesi aktif.
- `/exit` — Keluar dari aplikasi.

### 3. Mode One-Shot (Eksekusi Cepat)

Eksekusi tugas langsung dari terminal Anda:
```bash
# Menggunakan provider default
crootagent "Baca Cargo.toml lalu jelaskan dependensi yang digunakan"

# Tentukan provider dan model spesifik
crootagent --provider groq --model llama-3.3-70b-versatile "Buat file hello.py yang mencetak angka genap 1-20"

# Lewati konfirmasi interaktif (untuk automasi/skrip)
crootagent -y "Jalankan cargo test dan berikan ringkasannya"
```

### 4. Manajemen Sesi

```bash
# Melihat daftar seluruh sesi
crootagent session list

# Membuat sesi baru dengan nama
crootagent session new "Fitur-Auth"

# Menampilkan riwayat percakapan sesi
crootagent session show <SESSION_ID>

# Menghapus sesi
crootagent session delete <SESSION_ID>

# Ekspor sesi ke file
crootagent session export <SESSION_ID> --output log-sesi.md
```

---

## 🧪 Pengujian & Kualitas Perangkat Lunak

CRootAgent dilengkapi dengan *test suite* komprehensif yang mencakup pengujian unit pada konfigurasi, manajemen sesi, dan seluruh rangkaian perkakas lokal:

```bash
cargo test
```

Hasil pengujian otomatis dijalankan pada setiap commit melalui pipeline CI/CD GitHub Actions.

---

## 📄 Lisensi

Proyek ini dilisensikan di bawah **GNU General Public License v3.0 (GPL-3.0)**.  
Dikembangkan oleh: **Ahmad Zakky Nurfauzan** (NIM: **714250028**)  
Program Studi Sarjana Terapan Teknik Informatika (D4TI), Universitas Logistik dan Bisnis Internasional (ULBI), Bandung.
