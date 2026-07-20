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

/// Max USB MIDI output ports to poll.
#[cfg(target_os = "android")]
const MAX_PORTS: usize = 4;

/// Thread-safe wrapper for multiple AMidiOutputPort pointers.
/// Ports are added from JNI thread (Release), polled from audio callback (Acquire).
/// Supports up to MAX_PORTS ports simultaneously — no allocations.
#[cfg(target_os = "android")]
pub struct AmidiPort {
    ports: [AtomicPtr<AMidiOutputPort>; MAX_PORTS],
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
            ports: [
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
                AtomicPtr::new(ptr::null_mut()),
            ],
            device: AtomicPtr::new(ptr::null_mut()),
        }
    }

    /// Close all ports and release device. Call before opening new device.
    pub fn close_all(&self) {
        for slot in &self.ports {
            let port = slot.swap(ptr::null_mut(), Ordering::AcqRel);
            if !port.is_null() {
                unsafe { AMidiOutputPort_close(port); }
            }
        }
        let device = self.device.swap(ptr::null_mut(), Ordering::AcqRel);
        if !device.is_null() {
            unsafe { AMidiDevice_release(device); }
        }
    }

    /// Open a USB MIDI device and all its output ports from a Java MidiDevice.
    /// Called from JNI thread (not audio thread).
    /// `port_numbers` is the list of output port indices to open.
    pub fn open_device(
        &self,
        env: *mut jni::sys::JNIEnv,
        midi_device: jni::sys::jobject,
        port_numbers: &[i32],
    ) -> usize {
        self.close_all();

        let mut device: *mut AMidiDevice = ptr::null_mut();
        let status = unsafe { AMidiDevice_fromJava(env, midi_device, &mut device) };
        if status != 0 || device.is_null() {
            log::warn!("[amidi] AMidiDevice_fromJava failed: {status}");
            return 0;
        }
        self.device.store(device, Ordering::Release);

        let mut opened = 0usize;
        for (i, &port_num) in port_numbers.iter().enumerate() {
            if i >= MAX_PORTS { break; }

            let mut port: *mut AMidiOutputPort = ptr::null_mut();
            let status = unsafe { AMidiOutputPort_open(device, port_num, &mut port) };
            if status == 0 && !port.is_null() {
                self.ports[i].store(port, Ordering::Release);
                log::info!("[amidi] USB MIDI port {port_num} opened (slot {i})");
                opened += 1;
            } else {
                log::warn!("[amidi] AMidiOutputPort_open({port_num}) failed: {status}");
            }
        }

        opened
    }

    /// Poll ALL open ports for MIDI data. Non-blocking, RT-safe.
    /// Call from audio callback. Returns Some(nbytes) from first port with data.
    #[inline]
    pub fn receive(&self, buf: &mut [u8]) -> Option<usize> {
        for slot in &self.ports {
            let port = slot.load(Ordering::Acquire);
            if port.is_null() { continue; }

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
                return Some(nbytes);
            }
        }
        None
    }

    pub fn close(&self) {
        self.close_all();
    }
}

#[cfg(target_os = "android")]
impl Drop for AmidiPort {
    fn drop(&mut self) { self.close_all(); }
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
