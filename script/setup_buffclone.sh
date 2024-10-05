#!/bin/bash

# URL to download the buffclone.py script
SCRIPT_URL="https://raw.githubusercontent.com/narodnik/weechat-global-buffer/refs/heads/main/buffclone.py"

# Target directory for WeeChat Python autoload scripts
TARGET_DIR="$HOME/.local/share/weechat/python/autoload"

# Ensure the target directory exists, if not create it
if [ ! -d "$TARGET_DIR" ]; then
    echo "Creating target directory: $TARGET_DIR"
    mkdir -p "$TARGET_DIR"
fi

# Download the script and place it in the autoload directory
SCRIPT_PATH="$TARGET_DIR/buffclone.py"

echo "Downloading buffclone.py script..."
curl -o "$SCRIPT_PATH" "$SCRIPT_URL"

# Ensure the script has appropriate permissions
chmod +x "$SCRIPT_PATH"

echo "Script downloaded and placed in: $SCRIPT_PATH"

# Check if WeeChat is running, if so, reload the Python scripts
if pgrep weechat >/dev/null; then
    echo "WeeChat is running. Reloading Python scripts..."
    weechat --run-command="/python reload"
else
    echo "WeeChat is not running. The script will autoload when you start WeeChat."
fi

echo "Done."
