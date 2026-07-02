#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."

cargo ndk \
    -t arm64-v8a \
    -o android/app/src/main/jniLibs \
    build --release \
    --no-default-features \
    --features gui,android-app

echo "Done: android/app/src/main/jniLibs/arm64-v8a/libmini_midi_synth.so"
