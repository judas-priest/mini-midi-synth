#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

export ANDROID_NDK_HOME="${ANDROID_NDK_HOME:-/home/dima/.local/opt/android-sdk/ndk/28.2.13676358}"

cargo ndk \
    -t arm64-v8a \
    -P 30 \
    --link-libcxx-shared \
    -o android/app/src/main/jniLibs \
    build --release \
    --no-default-features \
    --features gui,android-app

# Keep the unstripped library so ndk-stack can symbolize crash dumps.
# Not committed — see .gitignore.
mkdir -p android/symbols/arm64-v8a
cp target/aarch64-linux-android/release/libmini_midi_synth.so \
   android/symbols/arm64-v8a/libmini_midi_synth.so

echo "Done: android/app/src/main/jniLibs/arm64-v8a/libmini_midi_synth.so"
echo "Symbols: android/symbols/arm64-v8a/libmini_midi_synth.so"
echo
echo "Symbolize a native crash with:"
echo "  adb logcat | \$ANDROID_NDK_HOME/prebuilt/linux-x86_64/bin/ndk-stack -sym android/symbols/arm64-v8a"
