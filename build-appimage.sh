#!/usr/bin/env bash
set -e

# ─── Colors ──────────────────────────────────────────────────────────────────
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

# ─── 1. Build binary ──────────────────────────────────────────────────────────
info "Building get-zen in release mode..."
cd "$SCRIPT_DIR"
cargo build --release
success "Build complete."

# ─── 2. Prepare AppDir ────────────────────────────────────────────────────────
info "Preparing AppDir..."
mkdir -p "$APPDIR/usr/bin"
mkdir -p "$APPDIR/usr/share/applications"
mkdir -p "$APPDIR/usr/share/icons/hicolor/256x256/apps"
mkdir -p "$APPDIR/usr/share/icons/hicolor/scalable/apps"

# Copy binary
cp "$SCRIPT_DIR/target/release/get-zen" "$APPDIR/usr/bin/get-zen"
chmod +x "$APPDIR/AppRun"
success "Binary copied."

# ─── 3. Convert SVG icon to PNG ───────────────────────────────────────────────
info "Converting SVG icon to PNG..."
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
        warning "No SVG→PNG converter found (rsvg-convert/inkscape/imagemagick)."
        warning "Skipping icon conversion; AppImage will use the SVG directly."
        cp "$svg" "$png" || true
        return 1
    fi
    return 0
}

convert_svg_to_png "$SVG_ICON" "$PNG_ICON" 256
convert_svg_to_png "$SVG_ICON" "$APPDIR/usr/share/icons/hicolor/256x256/apps/get-zen.png" 256
cp "$SVG_ICON" "$APPDIR/usr/share/icons/hicolor/scalable/apps/get-zen.svg"
cp "$APPDIR/get-zen.desktop" "$APPDIR/usr/share/applications/get-zen.desktop"
success "Icon copied."

# ─── 4. Download appimagetool if not present ─────────────────────────────────
if [ ! -f "$APPIMAGETOOL" ]; then
    info "Downloading appimagetool..."
    APPIMAGETOOL_URL="https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage"
    if command -v curl &>/dev/null; then
        curl -sL -o "$APPIMAGETOOL" "$APPIMAGETOOL_URL" || error "Failed to download appimagetool."
    elif command -v wget &>/dev/null; then
        wget -q -O "$APPIMAGETOOL" "$APPIMAGETOOL_URL" || error "Failed to download appimagetool."
    else
        error "curl or wget is required to download appimagetool."
    fi
    chmod +x "$APPIMAGETOOL"
    success "appimagetool downloaded."
else
    success "appimagetool already present."
fi

# ─── 5. Create AppImage ──────────────────────────────────────────────────────
info "Creating AppImage..."
cd "$SCRIPT_DIR"

# Required for FUSE in environments without native support
export APPIMAGE_EXTRACT_AND_RUN=1

"$APPIMAGETOOL" "$APPDIR" "$OUTPUT"

if [ -f "$OUTPUT" ]; then
    success "AppImage created successfully: $(basename "$OUTPUT")"
    echo ""
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}  get-zen-x86_64.AppImage ready to distribute    ${NC}"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    ls -lh "$OUTPUT"
else
    error "AppImage was not generated. Check the errors above."
fi
