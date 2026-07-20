package com.minimidisynth;

import android.media.midi.MidiOutputPort;
import android.media.midi.MidiReceiver;
import android.util.Log;

import java.io.IOException;

/**
 * Bridges Java MIDI data to native Rust code via JNI.
 * Attaches to a MidiOutputPort as a MidiReceiver, forwards
 * raw MIDI bytes to the native synth engine.
 */
public class MidiBridge extends MidiReceiver {
    private static final String TAG = "MiniMidiSynth";

    /** JNI native method — implemented in Rust (main.rs) */
    static native void onMidiData(byte[] data);

    private final MidiOutputPort mPort;
    private final String mDeviceName;

    public MidiBridge(MidiOutputPort port, String deviceName) {
        mPort = port;
        mDeviceName = deviceName;
    }

    @Override
    public void onSend(byte[] data, int offset, int count, long timestamp)
            throws IOException {
        Log.i(TAG, "onSend: " + count + " bytes from " + mDeviceName);
        if (count <= 0) return;

        byte[] slice;
        if (offset == 0 && count == data.length) {
            slice = data;
        } else {
            slice = new byte[count];
            System.arraycopy(data, offset, slice, 0, count);
        }

        try {
            onMidiData(slice);
        } catch (Exception e) {
            Log.e(TAG, "JNI onMidiData error", e);
        }
    }

    public void close() {
        try {
            mPort.close();
        } catch (IOException e) {
            Log.w(TAG, "Error closing MIDI port for " + mDeviceName, e);
        }
    }
}
