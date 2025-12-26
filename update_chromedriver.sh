#!/bin/bash

echo "🔍 Checking Chrome version..."
CHROME_VERSION=$(/Applications/Google\ Chrome.app/Contents/MacOS/Google\ Chrome --version | awk '{print $3}')
echo "   Chrome version: $CHROME_VERSION"

MAJOR_VERSION=$(echo $CHROME_VERSION | cut -d. -f1)
echo "   Major version: $MAJOR_VERSION"

echo ""
echo "📥 Downloading ChromeDriver for Chrome $MAJOR_VERSION..."

# Download the latest ChromeDriver for this Chrome version
if [ "$(uname -m)" = "arm64" ]; then
    PLATFORM="mac-arm64"
else
    PLATFORM="mac-x64"
fi

# ChromeDriver download URL for Chrome for Testing
DOWNLOAD_URL="https://googlechromelabs.github.io/chrome-for-testing/known-good-versions-with-downloads.json"

echo "   Platform: $PLATFORM"
echo "   Fetching download info..."

# Get the latest ChromeDriver version for this Chrome major version
CHROMEDRIVER_VERSION=$(curl -s "$DOWNLOAD_URL" | jq -r ".versions[] | select(.version | startswith(\"$MAJOR_VERSION.\")) | .version" | sort -V | tail -1)

if [ -z "$CHROMEDRIVER_VERSION" ]; then
    echo "❌ Could not find ChromeDriver for Chrome $MAJOR_VERSION"
    echo ""
    echo "Please manually download from:"
    echo "https://googlechromelabs.github.io/chrome-for-testing/"
    exit 1
fi

echo "   Found ChromeDriver version: $CHROMEDRIVER_VERSION"

# Construct download URL
CHROMEDRIVER_URL="https://storage.googleapis.com/chrome-for-testing-public/$CHROMEDRIVER_VERSION/$PLATFORM/chromedriver-$PLATFORM.zip"

echo ""
echo "📦 Downloading ChromeDriver $CHROMEDRIVER_VERSION..."
echo "   URL: $CHROMEDRIVER_URL"

# Download and extract
curl -L -o /tmp/chromedriver.zip "$CHROMEDRIVER_URL"

if [ $? -ne 0 ]; then
    echo "❌ Download failed"
    exit 1
fi

echo "📂 Extracting..."
unzip -o /tmp/chromedriver.zip -d /tmp/

# Move to /usr/local/bin or current directory
if [ -w "/usr/local/bin" ]; then
    echo "📍 Installing to /usr/local/bin..."
    mv /tmp/chromedriver-$PLATFORM/chromedriver /usr/local/bin/
    chmod +x /usr/local/bin/chromedriver
    INSTALL_PATH="/usr/local/bin/chromedriver"
else
    echo "📍 Installing to current directory..."
    mv /tmp/chromedriver-$PLATFORM/chromedriver ./
    chmod +x ./chromedriver
    INSTALL_PATH="./chromedriver"
fi

# Cleanup
rm -rf /tmp/chromedriver.zip /tmp/chromedriver-$PLATFORM

echo ""
echo "✅ ChromeDriver updated successfully!"
echo "   Version: $CHROMEDRIVER_VERSION"
echo "   Location: $INSTALL_PATH"
echo ""
echo "Now restart ChromeDriver:"
echo "   ./stop_chromedriver.sh"
echo "   ./start_chromedriver.sh"

