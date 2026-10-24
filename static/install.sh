#!/bin/sh
# 🦀 Ferris.uz Rasmiy CLI O'rnatuvchi Skripti (Linux x86_64)
set -e

RED=$(printf '\033[0;31m')
GREEN=$(printf '\033[0;32m')
YELLOW=$(printf '\033[1;33m')
CYAN=$(printf '\033[0;36m')
BOLD=$(printf '\033[1m')
NC=$(printf '\033[0m')

echo ""
echo "${CYAN}${BOLD}    _~^~^~_      🦀 Ferris.uz CLI O'rnatuvchisi${NC}"
echo "${CYAN}${BOLD}   ) / O O \\ (   O'zbekiston Rustaceanlar Hamjamiyati${NC}"
echo "${CYAN}${BOLD}  / /       \\ \\  Versiya: 0.3.0 (Landlock LSM + ChaCha20-Poly1305 Vault + PQC BLAKE3 + Secret Scanner + i18n)${NC}"
echo "${CYAN}${BOLD} / /_     _\\ \\   https://ferris.uz${NC}"
echo "${CYAN}${BOLD}|____)   (____|${NC}"
echo ""

# 1. Tizim va arxitekturani aniqlash
OS="$(uname -s)"
ARCH="$(uname -m)"

if [ "$OS" != "Linux" ]; then
    echo "${RED}❌ Xatolik: Ushbu o'rnatuvchi faqat Linux tizimlari uchun mo'ljallangan ($OS aniqlandi).${NC}"
    exit 1
fi

if [ "$ARCH" != "x86_64" ]; then
    echo "${YELLOW}⚠️ Eslatma: Hozirda faqat x86_64 arxitekturasi uchun binar mavjud ($ARCH aniqlandi).${NC}"
    echo "Kompilyatsiya qilish uchun: git clone https://github.com/iskurama/ferris-uz && cargo build --release"
fi

# 2. O'rnatish papkasini aniqlash
TARGET_DIR="/usr/local/bin"
if [ ! -w "$TARGET_DIR" ] || [ "$(id -u)" != "0" ]; then
    TARGET_DIR="$HOME/.local/bin"
    mkdir -p "$TARGET_DIR"
fi

echo "${CYAN}==>${NC} Ferris CLI binar fayli yuklab olinmoqda..."
DOWNLOAD_URL="https://ferris.uz/downloads/ferris-linux"
SHA_URL="https://ferris.uz/downloads/ferris-linux.sha256"

TMP_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t 'ferris-install')"
trap 'rm -rf "$TMP_DIR"' EXIT

if command -v curl >/dev/null 2>&1; then
    curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/ferris"
    curl -fsSL "$SHA_URL" -o "$TMP_DIR/ferris-linux.sha256" || true
elif command -v wget >/dev/null 2>&1; then
    wget -qO "$TMP_DIR/ferris" "$DOWNLOAD_URL"
    wget -qO "$TMP_DIR/ferris-linux.sha256" "$SHA_URL" || true
else
    echo "${RED}❌ Xatolik: curl yoki wget dasturi topilmadi!${NC}"
    exit 1
fi

# SHA-256 yaxlitlik tekshiruvi (agar mavjud bo'lsa)
if [ -s "$TMP_DIR/ferris-linux.sha256" ]; then
    echo "${CYAN}==>${NC} Binar yaxlitligi (SHA-256) tekshirilmoqda..."
    EXPECTED_SHA="$(awk '{print $1}' "$TMP_DIR/ferris-linux.sha256")"
    if command -v sha256sum >/dev/null 2>&1; then
        ACTUAL_SHA="$(sha256sum "$TMP_DIR/ferris" | awk '{print $1}')"
    elif command -v shasum >/dev/null 2>&1; then
        ACTUAL_SHA="$(shasum -a 256 "$TMP_DIR/ferris" | awk '{print $1}')"
    else
        ACTUAL_SHA=""
    fi

    if [ -n "$ACTUAL_SHA" ] && [ "$ACTUAL_SHA" != "$EXPECTED_SHA" ]; then
        echo "${RED}❌ Xavfsizlik xatosi: SHA-256 tekshiruvi muvaffaqiyatsiz bo'ldi! Yuklangan fayl zararlangan yoki to'liq emas.${NC}"
        exit 1
    elif [ -n "$ACTUAL_SHA" ]; then
        echo "${GREEN}✓ SHA-256 imzo tasdiqlandi: $ACTUAL_SHA${NC}"
    fi
fi

mv "$TMP_DIR/ferris" "$TARGET_DIR/ferris"
chmod +x "$TARGET_DIR/ferris"

echo ""
echo "${GREEN}${BOLD}✅ Muvaffaqiyatli o'rnatildi:${NC} $TARGET_DIR/ferris"

# 3. PATH tekshiruvi
case ":$PATH:" in
    *":$TARGET_DIR:"*) ;;
    *)
        echo ""
        echo "${YELLOW}⚠️ Diqqat:${NC} $TARGET_DIR katalogi sizning PATH o'zgaruvchingizda yo'q."
        echo "Buyruqni to'g'ridan-to'g'ri ishlatish uchun quyidagilarni bajaring:"
        echo "  ${BOLD}export PATH=\"$TARGET_DIR:\$PATH\"${NC}"
        echo "yoki uni ~/.bashrc yoki ~/.zshrc fayliga qo'shing."
        ;;
esac

echo ""
echo "${BOLD}Ishga tushirish uchun quyidagi buyruqlarni sinab ko'ring:${NC}"
echo "  ${CYAN}ferris ping${NC}        - Tizim va API holatini tekshirish"
echo "  ${CYAN}ferris feed${NC}        - Yangi post va muhokamalarni ko'rish"
echo "  ${CYAN}ferris live${NC}        - Jonli jamoat chatiga ulanish (WebSocket)"
echo "  ${CYAN}ferris terms${NC}       - Foydalanish qoidalarini o'qish"
echo "  ${CYAN}ferris --help${NC}      - Barcha buyruqlar ro'yxati"
echo ""
