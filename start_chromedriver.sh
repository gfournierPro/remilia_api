#!/bin/bash

# Start ChromeDriver for automatic re-authentication
# This script helps ensure ChromeDriver is running on the correct port

PORT=9515
PID_FILE=".chromedriver.pid"

echo "🌐 Starting ChromeDriver for auto-reauth..."
echo "📍 Port: $PORT"
echo ""

# Check if already running
if [ -f "$PID_FILE" ]; then
    OLD_PID=$(cat "$PID_FILE")
    if ps -p "$OLD_PID" > /dev/null 2>&1; then
        echo "⚠️  ChromeDriver is already running (PID: $OLD_PID)"
        echo "To restart, run: ./stop_chromedriver.sh"
        exit 1
    else
        rm "$PID_FILE"
    fi
fi

# Check if chromedriver is installed
if [ -f "./chromedriver" ]; then
    CHROMEDRIVER_BIN="./chromedriver"
    echo "ℹ️  Using local ChromeDriver: ./chromedriver"
elif command -v chromedriver &> /dev/null; then
    CHROMEDRIVER_BIN="chromedriver"
    echo "ℹ️  Using system ChromeDriver: $(which chromedriver)"
else
    echo "❌ ChromeDriver not found!"
    echo ""
    echo "Install it with:"
    echo "  macOS:  brew install chromedriver"
    echo "  Linux:  sudo apt install chromium-chromedriver"
    echo "  Or run: ./update_chromedriver.sh"
    echo "  Or download from: https://chromedriver.chromium.org/"
    exit 1
fi

# Start ChromeDriver in background
$CHROMEDRIVER_BIN --port=$PORT > chromedriver.log 2>&1 &
CHROME_PID=$!

# Save PID
echo $CHROME_PID > "$PID_FILE"

# Wait a moment to check if it started successfully
sleep 2

if ps -p "$CHROME_PID" > /dev/null 2>&1; then
    echo "✅ ChromeDriver started successfully!"
    echo "📝 PID: $CHROME_PID"
    echo "📋 Logs: chromedriver.log"
    echo ""
    echo "Now you can run:"
    echo "  cargo run --bin beetle_hunt_refactored"
    echo ""
    echo "To stop ChromeDriver:"
    echo "  ./stop_chromedriver.sh"
else
    echo "❌ Failed to start ChromeDriver"
    echo "Check chromedriver.log for details"
    rm "$PID_FILE"
    exit 1
fi
