#!/usr/bin/env bash
set -euo pipefail

BACKUP_DIR="/opt/ferris/backups"
ENV_FILE="/opt/ferris/.env"
LOG_FILE="/var/log/ferris_backup.log"
TIMESTAMP=$(date +"%Y-%m-%d_%H%M%S")
BACKUP_FILE="${BACKUP_DIR}/ferris_db_${TIMESTAMP}.sql.gz"

mkdir -p "${BACKUP_DIR}"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" | tee -a "${LOG_FILE}"
}

# 1. Muhit o'zgaruvchilarini yuklash
if [ -f "${ENV_FILE}" ]; then
    set -a
    # shellcheck disable=SC1090
    . "${ENV_FILE}"
    set +a
fi

DB_NAME="${DB_NAME:-ferris_db}"
DB_USER="${DB_USER:-ferris_user}"
DB_PASS="${DB_PASS:-ferris_secure_2026_pw}"
TG_TOKEN="${TELEGRAM_BOT_TOKEN:-8928247696:AAF_82P6DUzUQwAwqB5hJ_5RZW-PvkF0xig}"
TG_CHAT_ID="${TELEGRAM_ADMIN_CHAT_ID:-5415350162}"

log "🚀 PostgreSQL zaxira jarayoni boshlandi: ${DB_NAME}"

# 2. pg_dump va gzip -9 orqali arxivlash
if PGPASSWORD="${DB_PASS}" pg_dump -h 127.0.0.1 -U "${DB_USER}" "${DB_NAME}" | gzip -9 > "${BACKUP_FILE}"; then
    if gzip -t "${BACKUP_FILE}"; then
        SIZE=$(du -h "${BACKUP_FILE}" | cut -f1)
        log "✅ Zaxira fayli yaratildi va butunligi tekshirildi: ${BACKUP_FILE} (Hajmi: ${SIZE})"
    else
        log "❌ Xatolik: Zaxira fayli arxiv butunligi tekshiruvidan o'tmadi!"
        exit 1
    fi
else
    log "❌ Xatolik: pg_dump jarayoni muvaffaqiyatsiz tugadi!"
    exit 1
fi

# 3. Telegram Bot orqali jo'natish
log "📤 Zaxira fayli Telegram bot orqali jo'natilmoqda (Chat ID: ${TG_CHAT_ID})..."

CAPTION="📦 *Ferris.uz PostgreSQL Zaxira Nusxasi*
📅 *Sana:* $(date '+%Y-%m-%d %H:%M:%S') UTC
🗄 *Baza:* \`${DB_NAME}\`
📊 *Hajmi:* \`${SIZE}\`
🛡 *Arxiv butunligi:* Tasdiqlangan (gzip -t)
🧹 *Server holati:* Disk xotirasini tejash uchun serverdan o'chirildi."

TG_RESPONSE=$(curl -s -w "\n%{http_code}" -X POST "https://api.telegram.org/bot${TG_TOKEN}/sendDocument" \
    -F chat_id="${TG_CHAT_ID}" \
    -F document=@"${BACKUP_FILE}" \
    -F caption="${CAPTION}" \
    -F parse_mode="Markdown")

HTTP_STATUS=$(echo "${TG_RESPONSE}" | tail -n1)
BODY=$(echo "${TG_RESPONSE}" | sed '$d')

if [ "${HTTP_STATUS}" -eq 200 ] && echo "${BODY}" | grep -q '"ok":true'; then
    log "🎉 Zaxira fayli Telegram orqali muvaffaqiyatli yetkazildi!"
    
    # 4. Foydalanuvchi talabiga ko'ra disk to'lib qolmasligi uchun faylni serverdan o'chirish
    rm -f "${BACKUP_FILE}"
    log "🧹 Fayl muvaffaqiyatli jo'natilgach, diskdan o'chirildi: ${BACKUP_FILE}"
else
    log "⚠️ Ogohlantirish: Telegram bot orqali jo'natishda xatolik yuz berdi (HTTP ${HTTP_STATUS})! Fayl xavfsizlik uchun diskda saqlab qolindi: ${BODY}"
    exit 1
fi

log "✨ Barcha zaxira va tozalash jarayonlari muvaffaqiyatli yakunlandi."
