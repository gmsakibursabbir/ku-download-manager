#!/usr/bin/env sh
# Builds KuCore for Android (crates/ku-android → libkudroid.so) into
# app/src/main/jniLibs. Needs the Android NDK (ANDROID_NDK_HOME) and
#   cargo install cargo-ndk
#   rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
set -eu
here=$(cd "$(dirname "$0")/.." && pwd)
cd "$here/.."
profile=${1:-release}
cargo ndk \
  -t arm64-v8a -t armeabi-v7a -t x86_64 \
  --platform 24 \
  -o "$here/app/src/main/jniLibs" \
  build -p ku-android --profile "$profile"
node "$here/scripts/locales.mjs"
echo "native libraries: $here/app/src/main/jniLibs"
