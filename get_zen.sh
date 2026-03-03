#!/usr/bin/env bash

# Colores para mensajes
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # Sin color

# Función para descargar con reintentos
download_with_retry() {
    local url="$1"
    local output="$2"
    local max_attempts=5
    local wait_time=3
    
    for attempt in $(seq 1 $max_attempts); do
        echo -e "${BLUE}Intento $attempt de $max_attempts...${NC}"
        
        # Usar user-agent y headers para evitar bloqueos
        if curl -sL -A "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36" \
                -H "Accept: application/octet-stream" \
                --connect-timeout 10 \
                --max-time 300 \
                -o "$output" \
                "$url"; then
            
            # Verificar que el archivo se descargó correctamente
            if [ -s "$output" ]; then
                echo -e "${GREEN}Descarga exitosa.${NC}"
                return 0
            fi
        fi
        
        if [ $attempt -lt $max_attempts ]; then
            echo -e "${YELLOW}Fallo en la descarga. Esperando ${wait_time}s antes de reintentar...${NC}"
            sleep $wait_time
            wait_time=$((wait_time * 2))  # Backoff exponencial
        fi
    done
    
    echo -e "${RED}Error: No se pudo completar la descarga después de $max_attempts intentos.${NC}"
    return 1
}

# Verificar si se pasó el parámetro --yes o -y
SKIP_CONFIRM=false
if [[ "$1" == "--yes" ]] || [[ "$1" == "-y" ]]; then
    SKIP_CONFIRM=true
fi

echo -e "${BLUE}=== Instalador de Zen Browser ===${NC}"
echo ""
echo "Este script descargará e instalará Zen Browser automáticamente usando GearLever."
echo ""

# Solicitar confirmación solo si no se pasó --yes
if [ "$SKIP_CONFIRM" = false ]; then
    read -p "¿Deseas continuar con la instalación? (S/n): " -r
    echo ""

    # Si está vacío (Enter) o es S/s/Y/y, continuar
    if [[ -n $REPLY ]] && [[ ! $REPLY =~ ^[SsYy]$ ]]; then
        echo -e "${YELLOW}Instalación cancelada.${NC}"
        exit 0
    fi
else
    echo -e "${GREEN}Modo automático activado (--yes)${NC}"
fi

# Crear directorio temporal
TEMP_DIR=$(mktemp -d -t zen_install_XXXXXX)
echo -e "${BLUE}Creando directorio temporal: $TEMP_DIR${NC}"

cd "$TEMP_DIR" || exit 1

echo -e "${GREEN}Descargando Zen Browser...${NC}"
if ! download_with_retry "https://github.com/zen-browser/desktop/releases/latest/download/zen-x86_64.AppImage" "zen.AppImage"; then
    echo -e "${RED}Error: No se pudo descargar Zen Browser.${NC}"
    echo -e "${YELLOW}Tip: Si GitHub bloqueó tu IP, espera unos minutos e inténtalo de nuevo.${NC}"
    rm -rf "$TEMP_DIR"
    exit 1
fi

# Pequeña pausa entre descargas para evitar rate limiting
sleep 2

echo -e "${GREEN}Descargando GearLever...${NC}"
echo -e "${BLUE}Obteniendo URL de la última versión...${NC}"
GEAR_LEVER_URL=$(curl -s -A "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36" \
                      https://api.github.com/repos/pkgforge-dev/Gear-Lever-AppImage/releases/latest | \
                      grep -o '"browser_download_url": *"[^"]*x86_64\.AppImage"' | \
                      grep -o 'https://[^"]*')

if [ -z "$GEAR_LEVER_URL" ]; then
    echo -e "${RED}Error: No se pudo obtener la URL de GearLever.${NC}"
    rm -rf "$TEMP_DIR"
    exit 1
fi

if ! download_with_retry "$GEAR_LEVER_URL" "gear_lever.AppImage"; then
    echo -e "${RED}Error: No se pudo descargar GearLever.${NC}"
    echo -e "${YELLOW}Tip: Si GitHub bloqueó tu IP, espera unos minutos e inténtalo de nuevo.${NC}"
    rm -rf "$TEMP_DIR"
    exit 1
fi

# Dar permisos de ejecución
chmod +x zen.AppImage
chmod +x gear_lever.AppImage

echo -e "${GREEN}Integrando Zen Browser con GearLever...${NC}"
echo "y" | ./gear_lever.AppImage --integrate "$(pwd)/zen.AppImage"

if [ $? -eq 0 ]; then
    echo -e "${GREEN}¡Zen Browser instalado exitosamente!${NC}"
else
    echo -e "${YELLOW}Hubo un problema al integrar con GearLever.${NC}"
    echo "Los archivos se encuentran en: $TEMP_DIR"
    exit 1
fi

# Limpiar archivos temporales
echo -e "${BLUE}Limpiando archivos temporales...${NC}"
cd ~
rm -rf "$TEMP_DIR"

echo -e "${GREEN}¡Instalación completada! Zen Browser está listo para usar.${NC}"
