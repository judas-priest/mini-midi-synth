//! Direct AMidi NDK access for zero-latency USB MIDI in audio callback.
//! AMidiOutputPort_receive() is non-blocking and RT-safe per Google docs.

#[cfg(target_os = "android")]
use std::sync::atomic::{AtomicPtr, Ordering};
#[cfg(target_os = "android")]
use std::ptr;

#[cfg(target_os = "android")]
#[allow(non_camel_case_types)]
type media_status_t = i32;

#[cfg(target_os = "android")]
#[repr(C)]
struct AMidiDevice { _opaque: [u8; 0] }
#[cfg(target_os = "android")]
#[repr(C)]
pub struct AMidiOutputPort { _opaque: [u8; 0] }

#[cfg(target_os = "android")]
const AMIDI_OPCODE_DATA: i32 = 1;

#[cfg(target_os = "android")]
#[link(name = "amidi")]
extern "C" {
    fn AMidiDevice_fromJava(
        env: *mut jni::sys::JNIEnv,
        midi_device_obj: jni::sys::jobject,
        out_device: *mut *mut AMidiDevice,
    ) -> media_status_t;
    fn AMidiDevice_release(device: *mut AMidiDevice) -> media_status_t;
    fn AMidiOutputPort_open(
        device: *const AMidiDevice,
        port_number: i32,
        out_port: *mut *mut AMidiOutputPort,
    ) -> media_status_t;
    fn AMidiOutputPort_close(port: *mut AMidiOutputPort);
    fn AMidiOutputPort_receive(
        port: *mut AMidiOutputPort,
        opcode: *mut i32,
        buffer: *mut u8,
        max_bytes: usize,
        num_bytes: *mut usize,
        timestamp: *mut i64,
    ) -> isize;
}

/// Thread-safe wrapper for AMidiOutputPort pointer.
/// Set from JNI thread (Release), read from audio callback (Acquire).
#[cfg(target_os = "android")]
pub struct AmidiPort {
    port: AtomicPtr<AMidiOutputPort>,
    device: AtomicPtr<AMidiDevice>,
}

#[cfg(target_os = "android")]
// Safety: AMidiOutputPort_receive is documented as safe to call from any thread.
// Port/device pointers are only dereferenced through AMidi functions which are thread-safe.
unsafe impl Send for AmidiPort {}
#[cfg(target_os = "android")]
unsafe impl Sync for AmidiPort {}

#[cfg(target_os = "android")]
impl AmidiPort {
    pub const fn new() -> Self {
        Self {
            port: AtomicPtr::new(ptr::null_mut()),
            device: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Open AMidi port from a Java MidiDevice object.
    /// Called from JNI thread (not audio thread).
    pub fn open_from_java(
        &self,
        env: *mut jni::sys::JNIEnv,
        midi_device: jni::sys::jobject,
        port_number: i32,
    ) -> bool {
        // Atomically remove old port first (audio thread will see null and skip)
        let old_port = self.port.swap(ptr::null_mut(), Ordering::AcqRel);
        let old_device = self.device.swap(ptr::null_mut(), Ordering::AcqRel);

        // Close old resources after nulling pointers (audio thread no longer uses them)
        if !old_port.is_null() { unsafe { AMidiOutputPort_close(old_port); } }
        if !old_device.is_null() { unsafe { AMidiDevice_release(old_device); } }

        let mut device: *mut AMidiDevice = ptr::null_mut();
        let status = unsafe { AMidiDevice_fromJava(env, midi_device, &mut device) };
        if status != 0 || device.is_null() {
            log::warn!("[amidi] AMidiDevice_fromJava failed: {status}");
            return false;
        }

        let mut port: *mut AMidiOutputPort = ptr::null_mut();
        let status = unsafe { AMidiOutputPort_open(device, port_number, &mut port) };
        if status != 0 || port.is_null() {
            log::warn!("[amidi] AMidiOutputPort_open failed: {status}");
            unsafe { AMidiDevice_release(device); }
            return false;
        }

        // Store new pointers — audio thread will pick them up via Acquire load
        self.device.store(device, Ordering::Release);
        self.port.store(port, Ordering::Release);
        log::info!("[amidi] USB MIDI port {port_number} opened");
        true
    }

    /// Poll for MIDI data. Non-blocking, RT-safe.
    /// Call from audio callback. Returns num bytes or None.
    #[inline]
    pub fn receive(&self, buf: &mut [u8]) -> Option<usize> {
        let port = self.port.load(Ordering::Acquire);
        if port.is_null() { return None; }

        let mut opcode: i32 = 0;
        let mut nbytes: usize = 0;
        let mut timestamp: i64 = 0;

        let rc = unsafe {
            AMidiOutputPort_receive(
                port, &mut opcode, buf.as_mut_ptr(),
                buf.len(), &mut nbytes, &mut timestamp,
            )
        };

        if rc > 0 && opcode == AMIDI_OPCODE_DATA && nbytes > 0 {
            Some(nbytes)
        } else {
            None
        }
    }

    pub fn close(&self) {
        let port = self.port.swap(ptr::null_mut(), Ordering::AcqRel);
        if !port.is_null() { unsafe { AMidiOutputPort_close(port); } }
        let device = self.device.swap(ptr::null_mut(), Ordering::AcqRel);
        if !device.is_null() { unsafe { AMidiDevice_release(device); } }
    }
}

#[cfg(target_os = "android")]
impl Drop for AmidiPort {
    fn drop(&mut self) { self.close(); }
}

/// Stub for non-Android platforms — all methods are no-ops.
#[cfg(not(target_os = "android"))]
pub struct AmidiPort;

#[cfg(not(target_os = "android"))]
impl AmidiPort {
    pub const fn new() -> Self { Self }
    #[inline]
    pub fn receive(&self, _buf: &mut [u8]) -> Option<usize> { None }
    pub fn close(&self) {}
}
