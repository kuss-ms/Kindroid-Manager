#!/usr/bin/env bash
# Idempotent Android toolchain setup for Kindroid Manager.
# Required by the Android deployment plan (.kilo/plans/1785596514600-android-deployment-plan.md).
#
# Usage:
#   bash scripts/setup-android.sh         # install / verify, prints export hints
#   source scripts/setup-android.sh       # install / verify and ANDROID_HOME etc. into current shell
#
# Installs (or verifies) at $ANDROID_HOME:
#   - platform-tools
#   - platforms;android-33 (build-tools;33.0.2)
#   - ndk;29.0.14206865
#   - Rust targets: aarch64-linux-android, armv7-linux-androideabi, x86_64-linux-android
#
# Exports ANDROID_HOME / ANDROID_SDK_ROOT / ANDROID_NDK_HOME for the calling shell and
# prepends $ANDROID_HOME/platform-tools + cmdline-tools/latest/bin to PATH.

set -euo pipefail

# Pinned to match the Android 13 target phone and the SDK installed 2026-08-01.
NDK_VERSION="29.0.14206865"
ANDROID_PLATFORM="android-33"
BUILD_TOOLS_VERSION="33.0.2"

ANDROID_HOME="${ANDROID_HOME:-$HOME/android_sdk}"
ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-$ANDROID_HOME}"
ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-$ANDROID_HOME/ndk/$NDK_VERSION}"
SDKMANAGER="$ANDROID_HOME/cmdline-tools/latest/bin/sdkmanager"

log() { printf '\033[1;34m[setup-android]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[setup-android]\033[0m %s\n' "$*" >&2; }
die() { printf '\033[1;31m[setup-android]\033[0m %s\n' "$*" >&2; exit 1; }

# 1. JDK 17+ check (AGENTS.md requires JDK 17+; Android Gradle Plugin 8.x accepts up to 21).
if ! command -v java >/dev/null 2>&1; then
  die "java not found on PATH. Install JDK 17+ (e.g. \`sudo apt install openjdk-17-jdk\`) and re-run."
fi
JAVA_MAJOR="$(java -version 2>&1 | awk -F\" '/version/ {print $2}' | cut -d. -f1)"
if [ -z "$JAVA_MAJOR" ] || [ "$JAVA_MAJOR" -lt 17 ]; then
  die "Java $JAVA_MAJOR detected; JDK 17+ is required. Install openjdk-17-jdk and re-run with JAVA_HOME set."
fi
log "Java $(java -version 2>&1 | head -1) OK"

# 2. sdkmanager present (command-line tools).
if [ ! -x "$SDKMANAGER" ]; then
  die "sdkmanager not found at $SDKMANAGER. Install command-line tools under $ANDROID_HOME/cmdline-tools/latest/ first."
fi

# 3. SDK components (idempotent: sdkmanager skips already-installed packages).
log "Ensuring SDK components (platform-tools, platforms;$ANDROID_PLATFORM, build-tools;$BUILD_TOOLS_VERSION, ndk;$NDK_VERSION)"
yes | "$SDKMANAGER" --licenses >/dev/null
"$SDKMANAGER" \
  "platform-tools" \
  "platforms;$ANDROID_PLATFORM" \
  "build-tools;$BUILD_TOOLS_VERSION" \
  "ndk;$NDK_VERSION" >/dev/null

# 4. Rust Android targets.
for tgt in aarch64-linux-android armv7-linux-androideabi x86_64-linux-android; do
  if rustup target list --installed | grep -qx "$tgt"; then
    log "rust target $tgt already installed"
  else
    log "Installing rust target $tgt"
    rustup target add "$tgt"
  fi
done

# 5. Export env vars into the calling shell (works under both `bash` and `source`).
export ANDROID_HOME
export ANDROID_SDK_ROOT
export ANDROID_NDK_HOME
case ":$PATH:" in
  *":$ANDROID_HOME/platform-tools:"*) ;;
  *) export PATH="$ANDROID_HOME/platform-tools:$PATH" ;;
esac
case ":$PATH:" in
  *":$ANDROID_HOME/cmdline-tools/latest/bin:"*) ;;
  *) export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$PATH" ;;
esac

# When invoked (not sourced), also print the lines to put in ~/.bashrc / ~/.zshrc.
if [ "${BASH_SOURCE[0]:-}" = "$0" ]; then
  cat <<EOF

[setup-android] Done. Add these to your shell profile for future sessions:

  export ANDROID_HOME="$ANDROID_HOME"
  export ANDROID_SDK_ROOT="$ANDROID_SDK_ROOT"
  export ANDROID_NDK_HOME="$ANDROID_NDK_HOME"
  export PATH="\$PATH:$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmdline-tools/latest/bin"

[setup-android] Verify with:
  adb --version
  sdkmanager --list_installed | grep -E "platforms;$ANDROID_PLATFORM|build-tools;$BUILD_TOOLS_VERSION|ndk;$NDK_VERSION|platform-tools"
  rustup target list --installed
EOF
fi