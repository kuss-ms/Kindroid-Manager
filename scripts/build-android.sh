#!/usr/bin/env bash
# Build a signed Android release APK for Kindroid Manager.
# See .kilo/plans/1785596514600-android-deployment-plan.md step 7.
#
# Usage:
#   bash scripts/build-android.sh
#
# Prereqs (already in the toolchain status of the plan):
#   - $ANDROID_HOME, $ANDROID_SDK_ROOT, $ANDROID_NDK_HOME exported
#   - $ANDROID_HOME/platform-tools on PATH (for adb)
#   - JDK 17+ on PATH
#   - Rust Android targets installed (aarch64, armv7, x86_64)
#   - A keystore at ~/.keystores/kindroid-manager.jks with
#     src-tauri/gen/android/keystore.properties filled in
set -euo pipefail

: "${ANDROID_HOME:?ANDROID_HOME must be set}"
: "${ANDROID_NDK_HOME:?ANDROID_NDK_HOME must be set}"
export ANDROID_SDK_ROOT="${ANDROID_SDK_ROOT:-$ANDROID_HOME}"
case ":$PATH:" in
  *":$ANDROID_HOME/platform-tools:"*) ;;
  *) export PATH="$ANDROID_HOME/platform-tools:$PATH" ;;
esac

log() { printf '\033[1;34m[build-android]\033[0m %s\n' "$*"; }
die() { printf '\033[1;31m[build-android]\033[0m %s\n' "$*" >&2; exit 1; }

if [ ! -f src-tauri/gen/android/keystore.properties ]; then
  die "src-tauri/gen/android/keystore.properties is missing. Generate a keystore and create the properties file (see plan step 6)."
fi

log "Building frontend (pnpm build)"
pnpm build

log "Building signed Android release APK"
cd src-tauri && pnpm exec tauri android build --apk

APK="gen/android/app/build/outputs/apk/release/app-release.apk"
if [ -f "$APK" ]; then
  log "Done. APK at $(realpath "$APK")"
  log "Install with: adb install -r $APK"
else
  log "Build finished but $APK not found. Check the gradle output above."
fi
