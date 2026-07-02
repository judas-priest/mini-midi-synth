package com.minimidisynth;

import android.app.NativeActivity;
import android.os.Bundle;

/**
 * Thin wrapper around NativeActivity that pre-loads libc++_shared.so
 * before the native library is loaded. Without this, NativeActivity
 * fails with "cannot locate symbol __cxa_pure_virtual" because it
 * only loads the app's native lib, not its C++ runtime dependency.
 */
public class SynthActivity extends NativeActivity {
    static {
        System.loadLibrary("c++_shared");
    }
}
