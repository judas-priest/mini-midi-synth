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

echo "Done: android/app/src/main/jniLibs/arm64-v8a/libmini_midi_synth.so"
