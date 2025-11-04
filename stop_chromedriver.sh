#!/bin/bash

# Stop ChromeDriver

PID_FILE=".chromedriver.pid"

if [ ! -f "$PID_FILE" ]; then
    echo "❌ ChromeDriver PID file not found"
    echo "ChromeDriver might not be running, or was started manually"
    exit 1
fi

PID=$(cat "$PID_FILE")

if ps -p "$PID" > /dev/null 2>&1; then
    echo "🛑 Stopping ChromeDriver (PID: $PID)..."
    kill "$PID"
    rm "$PID_FILE"
    echo "✅ ChromeDriver stopped"
else
    echo "⚠️  Process $PID not found"
    echo "Cleaning up PID file..."
    rm "$PID_FILE"
fi
