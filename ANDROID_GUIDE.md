# Running on Android (Xiaomi Phone)

This guide explains how to run the compiled binaries on your Xiaomi phone.

## Prerequisites

You need one of the following:
- **Option 1**: Termux app (recommended for ease of use)
- **Option 2**: ADB (Android Debug Bridge) with USB debugging enabled

## Option 1: Using Termux (Recommended)

Termux is a terminal emulator for Android that doesn't require root access.

### Step 1: Install Termux
Download Termux from:
- [F-Droid](https://f-droid.org/packages/com.termux/) (recommended)
- [GitHub Releases](https://github.com/termux/termux-app/releases)

⚠️ **Don't use the Play Store version** - it's outdated and no longer maintained.

### Step 2: Transfer the Binary

1. Download the binary from GitHub:
   - Go to Actions → Cross-compile for Android → Latest successful run
   - Download the artifact (e.g., `beetle_hunt-android`)
   
2. Transfer to your phone:
   - Use a file manager to move it to your Downloads folder
   - Or transfer via USB/cloud storage

### Step 3: Run in Termux

```bash
# Open Termux and navigate to where you saved the binary
cd /sdcard/Download

# Make it executable
chmod +x beetle_hunt-android

# Run it
./beetle_hunt-android
```

### Step 4: Setup Storage Access (if needed)

If you need to access files outside Termux:
```bash
termux-setup-storage
```

This grants Termux permission to access your phone's storage.

## Option 2: Using ADB

This requires a computer and USB debugging enabled on your phone.

### Step 1: Enable USB Debugging

1. Go to Settings → About Phone
2. Tap "MIUI Version" 7 times to enable Developer Options
3. Go to Settings → Additional Settings → Developer Options
4. Enable "USB Debugging"

### Step 2: Connect and Transfer

```bash
# On your computer
# Verify connection
adb devices

# Transfer the binary
adb push beetle_hunt-android /data/local/tmp/beetle_hunt

# Make it executable
adb shell chmod +x /data/local/tmp/beetle_hunt

# Run it
adb shell /data/local/tmp/beetle_hunt
```

## Running Different Binaries

The workflow creates three binaries:

### Beetle Hunt (Original)
```bash
./beetle_hunt-android
```

### Beetle Hunt Refactored
```bash
./beetle_hunt_refactored-android
```

### Leaderboard Generator
```bash
./leaderboard-android
```

## Troubleshooting

### "Permission denied" error
Make sure the file is executable:
```bash
chmod +x beetle_hunt-android
```

### "No such file or directory" error
Verify you're in the correct directory:
```bash
pwd  # Print current directory
ls   # List files
```

### Binary crashes or doesn't work
- Ensure your phone is ARM64 (most modern Xiaomi phones are)
- Check if you need to install additional libraries in Termux:
```bash
pkg update
pkg upgrade
pkg install openssl
```

### Network/Cookie Issues
The binaries may need access to:
- `auth.txt` file
- `remilia_cookies.json` file

Make sure these are in the same directory as the binary, or adjust the paths in your configuration.

## Performance Notes

- Android binaries may run slower than native desktop versions
- Battery usage might be higher during intensive operations
- Use a charger for long-running tasks
- Monitor your phone's temperature

## Getting the Latest Binary

The binaries are automatically updated whenever you push code changes:
1. Go to your GitHub repository
2. Navigate to the `binaries/android/` directory
3. Download the latest binary
4. Transfer to your phone

Or download from the Actions artifacts:
1. Go to Actions tab
2. Click on the latest "Cross-compile for Android" workflow
3. Download the artifact

## Security Note

Only run binaries you've compiled yourself or from trusted sources. These binaries have full access to your phone's resources (within Android's security model).
