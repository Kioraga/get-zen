#!/usr/bin/env bash
set -e

# ─── Colores ──────────────────────────────────────────────────────────────────
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

info()    { echo -e "${BLUE}▶ $*${NC}"; }
success() { echo -e "${GREEN}✔ $*${NC}"; }
warning() { echo -e "${YELLOW}⚠ $*${NC}"; }
error()   { echo -e "${RED}✖ $*${NC}"; exit 1; }

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
APPDIR="$SCRIPT_DIR/AppDir"
ASSETS="$SCRIPT_DIR/assets"
OUTPUT="$SCRIPT_DIR/get-zen-x86_64.AppImage"
APPIMAGETOOL="$SCRIPT_DIR/appimagetool-x86_64.AppImage"

# ─── 1. Compilar binario ──────────────────────────────────────────────────────
info "Compilando get-zen en modo release..."
cd "$SCRIPT_DIR"
cargo build --release
success "Compilación completada."

# ─── 2. Preparar AppDir ───────────────────────────────────────────────────────
info "Preparando AppDir..."
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"
mkdir -p "$APPDIR/usr/share/icons/hicolor/scalable/apps"

# Copiar binario
cp "$SCRIPT_DIR/target/release/get-zen" "$APPDIR/usr/bin/get-zen"
chmod +x "$APPDIR/AppRun"
success "Binario copiado."

# ─── 3. Convertir icono SVG a PNG ─────────────────────────────────────────────
info "Convirtiendo icono SVG a PNG..."
SVG_ICON="$ASSETS/get-zen.svg"
PNG_ICON="$APPDIR/get-zen.png"

convert_svg_to_png() {
    local svg="$1" png="$2" size="${3:-256}"
    if command -v rsvg-convert &>/dev/null; then
        rsvg-convert -w "$size" -h "$size" -o "$png" "$svg"
    elif command -v inkscape &>/dev/null; then
        inkscape --export-png="$png" --export-width="$size" --export-height="$size" "$svg" 2>/dev/null
    elif command -v convert &>/dev/null; then
        convert -background none -resize "${size}x${size}" "$svg" "$png"
    elif command -v magick &>/dev/null; then
        magick -background none -resize "${size}x${size}" "$svg" "$png"
    else
        warning "No se encontró convertidor SVG→PNG (rsvg-convert/inkscape/imagemagick)."
        warning "Saltando conversión de icono; el AppImage usará el SVG directamente."
        cp "$svg" "$png" || true
        return 1
    fi
    return 0
}

convert_svg_to_png "$SVG_ICON" "$PNG_ICON" 256
convert_svg_to_png "$SVG_ICON" "$APPDIR/usr/share/icons/hicolor/256x256/apps/get-zen.png" 256
cp "$SVG_ICON" "$APPDIR/usr/share/icons/hicolor/scalable/apps/get-zen.svg"
cp "$APPDIR/get-zen.desktop" "$APPDIR/usr/share/applications/get-zen.desktop"
success "Icono copiado."

# ─── 4. Descargar appimagetool si no está presente ───────────────────────────
if [ ! -f "$APPIMAGETOOL" ]; then
    info "Descargando appimagetool..."
    APPIMAGETOOL_URL="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
    if command -v curl &>/dev/null; then
        curl -sL -o "$APPIMAGETOOL" "$APPIMAGETOOL_URL" || error "No se pudo descargar appimagetool."
    elif command -v wget &>/dev/null; then
        wget -q -O "$APPIMAGETOOL" "$APPIMAGETOOL_URL" || error "No se pudo descargar appimagetool."
    else
        error "Se necesita curl o wget para descargar appimagetool."
    fi
    chmod +x "$APPIMAGETOOL"
    success "appimagetool descargado."
else
    success "appimagetool ya presente."
fi

# ─── 5. Crear AppImage ────────────────────────────────────────────────────────
info "Creando AppImage..."
cd "$SCRIPT_DIR"

# Necesario para FUSE en entornos sin soporte nativo
export APPIMAGE_EXTRACT_AND_RUN=1

"$APPIMAGETOOL" "$APPDIR" "$OUTPUT"

if [ -f "$OUTPUT" ]; then
    success "AppImage creada exitosamente: $(basename "$OUTPUT")"
    echo ""
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}  get-zen-x86_64.AppImage lista para distribuir  ${NC}"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    ls -lh "$OUTPUT"
else
    error "No se generó el AppImage. Revisa los errores anteriores."
fi
