package com.minimidisynth;

import android.app.NativeActivity;
import android.bluetooth.BluetoothAdapter;
import android.bluetooth.BluetoothDevice;
import android.bluetooth.BluetoothManager;
import android.content.Context;
import android.content.pm.PackageManager;
import android.media.midi.MidiDevice;
import android.media.midi.MidiDeviceInfo;
import android.media.midi.MidiManager;
import android.media.midi.MidiOutputPort;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.util.Log;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;

/**
 * Wrapper around NativeActivity:
 * 1) Pre-loads libc++_shared.so (C++ runtime for cpal/Oboe)
 * 2) Opens paired Bluetooth MIDI devices via MidiManager
 * 3) Connects MidiBridge (Java MidiReceiver → JNI → Rust) to each device
 */
public class SynthActivity extends NativeActivity {
    private static final String TAG = "MiniMidiSynth";

    static {
        System.loadLibrary("c++_shared");
        System.loadLibrary("mini_midi_synth");
    }

    private MidiManager mMidiManager;

    // BLE MIDI auto-reconnect with exponential backoff
    private final Handler mReconnectHandler = new Handler(Looper.getMainLooper());
    private int mReconnectAttempts = 0;
    private static final int MAX_RECONNECT_ATTEMPTS = 10;
    private static final long RECONNECT_BASE_DELAY_MS = 3000;

    private final Runnable mReconnectRunnable = new Runnable() {
        @Override
        public void run() {
            if (mReconnectAttempts >= MAX_RECONNECT_ATTEMPTS) {
                Log.i(TAG, "BLE reconnect: max attempts reached, stopping");
                return;
            }
            mReconnectAttempts++;
            Log.i(TAG, "BLE reconnect attempt " + mReconnectAttempts);
            openPairedBluetoothMidiDevices();
            long delay = Math.min(RECONNECT_BASE_DELAY_MS * (1L << (mReconnectAttempts - 1)), 30000);
            mReconnectHandler.postDelayed(this, delay);
        }
    };

    // USB MIDI hot-plug
    private MidiManager.DeviceCallback mDeviceCallback;
    // All access to mOpenDevices and mBridges happens on the main looper thread:
    // - onCreate (main)
    // - onDestroy (main)
    // - OnDeviceOpenedListener callback (posted to main looper via handler)
    // No synchronization needed.
    private final List<MidiDevice> mOpenDevices = new ArrayList<>();
    private final List<MidiBridge> mBridges = new ArrayList<>();

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        getWindow().addFlags(android.view.WindowManager.LayoutParams.FLAG_KEEP_SCREEN_ON);
        // MIDI init AFTER super.onCreate() — Activity context is now fully available.
        // android_main runs in a separate thread, so it won't block on this.
        openPairedBluetoothMidiDevices();
        mReconnectHandler.postDelayed(mReconnectRunnable, RECONNECT_BASE_DELAY_MS);
        openUsbMidiDevices();
        registerMidiDeviceCallback();
    }

    @Override
    protected void onPause() {
        super.onPause();
        setAudioPaused(true);
        Log.i(TAG, "onPause — audio paused");
    }

    @Override
    protected void onResume() {
        super.onResume();
        setAudioPaused(false);
        Log.i(TAG, "onResume — audio resumed");
    }

    @Override
    protected void onDestroy() {
        mReconnectHandler.removeCallbacks(mReconnectRunnable);
        if (mMidiManager != null && mDeviceCallback != null) {
            mMidiManager.unregisterDeviceCallback(mDeviceCallback);
        }
        for (MidiBridge bridge : mBridges) {
            bridge.close();
        }
        mBridges.clear();
        for (MidiDevice dev : mOpenDevices) {
            try { dev.close(); } catch (Exception e) {
                Log.w(TAG, "Error closing MIDI device", e);
            }
        }
        mOpenDevices.clear();
        super.onDestroy();
    }

    private void openPairedBluetoothMidiDevices() {
        mMidiManager = (MidiManager) getSystemService(Context.MIDI_SERVICE);
        if (mMidiManager == null) {
            Log.w(TAG, "MidiManager not available");
            return;
        }

        BluetoothManager btManager = (BluetoothManager) getSystemService(Context.BLUETOOTH_SERVICE);
        if (btManager == null) {
            Log.w(TAG, "BluetoothManager not available");
            return;
        }

        BluetoothAdapter adapter = btManager.getAdapter();
        if (adapter == null || !adapter.isEnabled()) {
            Log.w(TAG, "Bluetooth not enabled");
            return;
        }

        if (checkSelfPermission(android.Manifest.permission.BLUETOOTH_CONNECT)
                != PackageManager.PERMISSION_GRANTED) {
            Log.i(TAG, "Requesting BLUETOOTH_CONNECT permission");
            // Post to handler — requesting permissions before Activity is fully
            // visible can cause issues on some devices
            new Handler(Looper.getMainLooper()).post(() ->
                requestPermissions(
                    new String[]{android.Manifest.permission.BLUETOOTH_CONNECT}, 1));
            return;
        }

        openBondedDevices(adapter);
    }

    @Override
    public void onRequestPermissionsResult(int requestCode, String[] permissions, int[] grantResults) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults);
        if (requestCode == 1 && grantResults.length > 0
                && grantResults[0] == PackageManager.PERMISSION_GRANTED) {
            BluetoothManager btManager = (BluetoothManager) getSystemService(Context.BLUETOOTH_SERVICE);
            if (btManager != null && btManager.getAdapter() != null) {
                openBondedDevices(btManager.getAdapter());
            }
        }
    }

    private void openBondedDevices(BluetoothAdapter adapter) {
        Set<BluetoothDevice> bonded = adapter.getBondedDevices();
        if (bonded == null || bonded.isEmpty()) {
            Log.i(TAG, "No bonded Bluetooth devices");
            return;
        }

        Handler handler = new Handler(Looper.getMainLooper());
        for (BluetoothDevice device : bonded) {
            String name = device.getName();
            Log.i(TAG, "Trying BLE MIDI: " + name + " [" + device.getAddress() + "]");
            try {
                mMidiManager.openBluetoothDevice(device,
                    new MidiManager.OnDeviceOpenedListener() {
                        @Override
                        public void onDeviceOpened(MidiDevice midiDevice) {
                            if (midiDevice == null) {
                                Log.w(TAG, "BLE MIDI failed: " + name);
                                return;
                            }
                            mOpenDevices.add(midiDevice);
                            Log.i(TAG, "BLE MIDI opened: " + name);
                            connectBridge(midiDevice, name);
                        }
                    }, handler);
            } catch (Exception e) {
                Log.w(TAG, "openBluetoothDevice error for " + name, e);
            }
        }
    }

    private static native void openUsbMidiNative(MidiDevice device, int[] portNumbers);
    private static native void closeUsbMidiNative();
    private static native void setAudioPaused(boolean paused);

    /**
     * Open already-connected USB MIDI devices via MidiManager.
     * Unlike BLE, USB devices are available immediately without openBluetoothDevice().
     */
    private void openUsbMidiDevices() {
        if (mMidiManager == null) {
            mMidiManager = (MidiManager) getSystemService(Context.MIDI_SERVICE);
        }
        if (mMidiManager == null) return;

        MidiDeviceInfo[] infos = mMidiManager.getDevices();
        for (MidiDeviceInfo info : infos) {
            if (info.getType() != MidiDeviceInfo.TYPE_USB) continue;

            String name = info.getProperties().getString(MidiDeviceInfo.PROPERTY_NAME);
            if (name == null) name = "USB MIDI";
            Log.i(TAG, "Found USB MIDI: " + name);

            String finalName = name;
            mMidiManager.openDevice(info, new MidiManager.OnDeviceOpenedListener() {
                @Override
                public void onDeviceOpened(MidiDevice midiDevice) {
                    if (midiDevice == null) {
                        Log.w(TAG, "USB MIDI open failed: " + finalName);
                        return;
                    }
                    mOpenDevices.add(midiDevice);
                    Log.i(TAG, "USB MIDI opened: " + finalName);

                    // Collect all output port numbers, open via AMidi in one call
                    MidiDeviceInfo.PortInfo[] ports = midiDevice.getInfo().getPorts();
                    java.util.List<Integer> outputPorts = new java.util.ArrayList<>();
                    for (MidiDeviceInfo.PortInfo pi : ports) {
                        if (pi.getType() == MidiDeviceInfo.PortInfo.TYPE_OUTPUT) {
                            outputPorts.add(pi.getPortNumber());
                        }
                    }
                    if (!outputPorts.isEmpty()) {
                        int[] portArray = outputPorts.stream().mapToInt(Integer::intValue).toArray();
                        Log.i(TAG, "AMidi: opening " + portArray.length + " output ports");
                        openUsbMidiNative(midiDevice, portArray);
                    }
                }
            }, new Handler(Looper.getMainLooper()));
        }
    }

    private void registerMidiDeviceCallback() {
        if (mMidiManager == null) return;

        mDeviceCallback = new MidiManager.DeviceCallback() {
            @Override
            public void onDeviceAdded(MidiDeviceInfo info) {
                if (info.getType() == MidiDeviceInfo.TYPE_USB) {
                    String name = info.getProperties().getString(MidiDeviceInfo.PROPERTY_NAME);
                    Log.i(TAG, "USB MIDI hot-plug: connected " + name);
                    openUsbMidiDevices();
                }
            }

            @Override
            public void onDeviceRemoved(MidiDeviceInfo info) {
                if (info.getType() == MidiDeviceInfo.TYPE_USB) {
                    String name = info.getProperties().getString(MidiDeviceInfo.PROPERTY_NAME);
                    Log.i(TAG, "USB MIDI hot-plug: disconnected " + name);
                    closeUsbMidiNative();
                }
            }
        };

        mMidiManager.registerDeviceCallback(mDeviceCallback, new Handler(Looper.getMainLooper()));
        Log.i(TAG, "USB MIDI hot-plug callback registered");
    }

    private void connectBridge(MidiDevice device, String name) {
        MidiDeviceInfo info = device.getInfo();
        MidiDeviceInfo.PortInfo[] ports = info.getPorts();
        for (MidiDeviceInfo.PortInfo pi : ports) {
            // TYPE_OUTPUT = data FROM device TO app (e.g. key presses)
            if (pi.getType() == MidiDeviceInfo.PortInfo.TYPE_OUTPUT) {
                MidiOutputPort outPort = device.openOutputPort(pi.getPortNumber());
                if (outPort != null) {
                    MidiBridge bridge = new MidiBridge(outPort, name);
                    outPort.connect(bridge);
                    mBridges.add(bridge);
                    mReconnectAttempts = 0; // Reset backoff on success
                    Log.i(TAG, "BLE MIDI bridge: " + name + " port " + pi.getPortNumber());
                }
            }
        }
    }
}
