#!/usr/bin/env bash

# Colors for messages
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No color

# Function to download with retries
download_with_retry() {
    local url="$1"
    local output="$2"
    local max_attempts=5
    local wait_time=3
    
    for attempt in $(seq 1 $max_attempts); do
        echo -e "${BLUE}Attempt $attempt of $max_attempts...${NC}"
        
        # Use user-agent and headers to avoid blocks
        if curl -sL -A "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36" \
                -H "Accept: application/octet-stream" \
                --connect-timeout 10 \
                --max-time 300 \
                -o "$output" \
                "$url"; then
            
            # Verify the file was downloaded correctly
            if [ -s "$output" ]; then
                echo -e "${GREEN}Download successful.${NC}"
                return 0
            fi
        fi
        
        if [ $attempt -lt $max_attempts ]; then
            echo -e "${YELLOW}Download failed. Waiting ${wait_time}s before retrying...${NC}"
            sleep $wait_time
            wait_time=$((wait_time * 2))  # Exponential backoff
        fi
    done
    
    echo -e "${RED}Error: Failed to complete download after $max_attempts attempts.${NC}"
    return 1
}

# Check if --yes or -y was passed
SKIP_CONFIRM=false
if [[ "$1" == "--yes" ]] || [[ "$1" == "-y" ]]; then
    SKIP_CONFIRM=true
fi

echo -e "${BLUE}=== Zen Browser Installer ===${NC}"
echo ""
echo "This script will download and install Zen Browser automatically using GearLever."
echo ""

# Ask for confirmation only if --yes was not passed
if [ "$SKIP_CONFIRM" = false ]; then
    read -p "Do you want to continue with the installation? (Y/n): " -r
    echo ""

    # If empty (Enter) or Y/y/S/s, continue
    if [[ -n $REPLY ]] && [[ ! $REPLY =~ ^[SsYy]$ ]]; then
        echo -e "${YELLOW}Installation cancelled.${NC}"
        exit 0
    fi
else
    echo -e "${GREEN}Automatic mode activated (--yes)${NC}"
fi

# Create temporary directory
TEMP_DIR=$(mktemp -d -t zen_install_XXXXXX)
echo -e "${BLUE}Creating temporary directory: $TEMP_DIR${NC}"

cd "$TEMP_DIR" || exit 1

echo -e "${GREEN}Downloading Zen Browser...${NC}"
if ! download_with_retry "https://github.com/zen-browser/desktop/releases/latest/download/zen-x86_64.AppImage" "zen.AppImage"; then
    echo -e "${RED}Error: Failed to download Zen Browser.${NC}"
    echo -e "${YELLOW}Tip: If GitHub blocked your IP, wait a few minutes and try again.${NC}"
    rm -rf "$TEMP_DIR"
    exit 1
fi

# Small pause between downloads to avoid rate limiting
sleep 2

echo -e "${GREEN}Downloading GearLever...${NC}"
echo -e "${BLUE}Getting URL of the latest version...${NC}"

GEAR_LEVER_URL=""
for attempt in $(seq 1 5); do
    echo -e "${BLUE}Attempt $attempt of 5 (GitHub API)...${NC}"
    GEAR_LEVER_URL=$(curl -s \
                          -A "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36" \
                          -H "Accept: application/vnd.github+json" \
                          --connect-timeout 10 \
                          --max-time 30 \
                          https://api.github.com/repos/pkgforge-dev/Gear-Lever-AppImage/releases/latest | \
                          grep -o '"browser_download_url": *"[^"]*x86_64\.AppImage"' | \
                          grep -o 'https://[^"]*')
    if [ -n "$GEAR_LEVER_URL" ]; then
        break
    fi
    if [ "$attempt" -lt 5 ]; then
        WAIT=$((attempt * 3))
        echo -e "${YELLOW}No response received. Waiting ${WAIT}s before retrying...${NC}"
        sleep $WAIT
    fi
done

if [ -z "$GEAR_LEVER_URL" ]; then
    echo -e "${RED}Error: Failed to get GearLever URL.${NC}"
    echo -e "${YELLOW}Tip: GitHub API may be rate limiting requests. Wait a few minutes and try again.${NC}"
    rm -rf "$TEMP_DIR"
    exit 1
fi

if ! download_with_retry "$GEAR_LEVER_URL" "gear_lever.AppImage"; then
    echo -e "${RED}Error: Failed to download GearLever.${NC}"
    echo -e "${YELLOW}Tip: If GitHub blocked your IP, wait a few minutes and try again.${NC}"
    rm -rf "$TEMP_DIR"
    exit 1
fi

# Grant execution permissions
chmod +x zen.AppImage
chmod +x gear_lever.AppImage

echo -e "${GREEN}Integrating Zen Browser with GearLever...${NC}"
echo "y" | ./gear_lever.AppImage --integrate "$(pwd)/zen.AppImage"

if [ $? -eq 0 ]; then
    echo -e "${GREEN}Zen Browser installed successfully!${NC}"
else
    echo -e "${YELLOW}There was a problem integrating with GearLever.${NC}"
    echo "Files are located at: $TEMP_DIR"
    exit 1
fi

# Clean up temporary files
echo -e "${BLUE}Cleaning up temporary files...${NC}"
cd ~
rm -rf "$TEMP_DIR"

echo -e "${GREEN}Installation complete! Zen Browser is ready to use.${NC}"
