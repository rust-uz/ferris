#!/usr/bin/env bash
set -euo pipefail

BACKUP_DIR="/opt/ferris/backups"
DB_NAME="ferris_db"
DB_USER="ferris_user"
TIMESTAMP=$(date +"%Y-%m-%d_%H%M%S")
BACKUP_FILE="${BACKUP_DIR}/ferris_db_${TIMESTAMP}.sql.gz"
LOG_FILE="/var/log/ferris_backup.log"
RETENTION_DAYS=14

mkdir -p "${BACKUP_DIR}"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" | tee -a "${LOG_FILE}"
}

log "🚀 PostgreSQL zaxira jarayoni boshlandi: ${DB_NAME}"

if PGPASSWORD="ferris_secure_2026_pw" pg_dump -h 127.0.0.1 -U "${DB_USER}" "${DB_NAME}" | gzip -9 > "${BACKUP_FILE}"; then
    if gzip -t "${BACKUP_FILE}"; then
        SIZE=$(du -h "${BACKUP_FILE}" | cut -f1)
        log "✅ Zaxira muvaffaqiyatli yakunlandi: ${BACKUP_FILE} (Hajmi: ${SIZE})"
    else
        log "❌ Xatolik: Zaxira fayli arxiv butunligi tekshiruvidan o'tmadi!"
        exit 1
    fi
else
    log "❌ Xatolik: pg_dump jarayoni muvaffaqiyatsiz tugadi!"
    exit 1
fi

DELETED_COUNT=0
while IFS= read -r old_backup; do
    if [ -n "${old_backup}" ]; then
        rm -f "${old_backup}"
        DELETED_COUNT=$((DELETED_COUNT + 1))
    fi
done < <(find "${BACKUP_DIR}" -name "ferris_db_*.sql.gz" -type f -mtime +"${RETENTION_DAYS}")

if [ "${DELETED_COUNT}" -gt 0 ]; then
    log "🧹 ${RETENTION_DAYS} kundan eski ${DELETED_COUNT} ta zaxira nusxasi tozalandi."
fi

log "✨ Zaxira jarayoni to'liq yakunlandi."
