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
    }

    private MidiManager mMidiManager;
    private final List<MidiDevice> mOpenDevices = new ArrayList<>();
    private final List<MidiBridge> mBridges = new ArrayList<>();

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        openPairedBluetoothMidiDevices();
        super.onCreate(savedInstanceState);
    }

    @Override
    protected void onDestroy() {
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
            requestPermissions(
                new String[]{android.Manifest.permission.BLUETOOTH_CONNECT}, 1);
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
                    Log.i(TAG, "BLE MIDI bridge: " + name + " port " + pi.getPortNumber());
                }
            }
        }
    }
}
