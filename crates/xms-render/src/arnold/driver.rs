//! Imago's display driver, installed in Arnold as a node type of its own: each
//! bucket's pixels, as Arnold finishes it, into memory (`Pixels`), so the image
//! refines in the pane while a pass renders. Arnold calls it from its render
//! threads; it is a table of functions, laid out as `ai_drivers.h` and
//! `ai_node_entry.h` declare them.

use std::ffi::{c_char, c_int, c_void};
use std::sync::{Arc, Mutex, OnceLock};

use super::ffi::{self, AtString, Ptr};
use crate::Pixels;

/// The driver's node type.
pub const NAME: &std::ffi::CStr = c"xms_imago_pixels";

/// The image it fills: one Arnold per process, one image.
static PIXELS: Mutex<Option<Arc<Pixels>>> = Mutex::new(None);

/// The calls into Arnold it makes, set when it is installed.
pub struct Calls {
    pub driver_initialize: unsafe extern "C" fn(Ptr, bool),
    pub output_next: unsafe extern "C" fn(Ptr, *mut AtString, *mut c_int, *mut *const c_void) -> bool,
}

static CALLS: OnceLock<Calls> = OnceLock::new();

/// `AtBBox2`: inclusive pixel bounds.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BBox2 {
    minx: c_int,
    miny: c_int,
    maxx: c_int,
    maxy: c_int,
}

/// `AtCommonMethods`.
#[repr(C)]
pub struct CommonMethods {
    plugin_initialize: unsafe extern "C" fn(*mut Ptr) -> bool,
    plugin_cleanup: unsafe extern "C" fn(Ptr),
    parameters: unsafe extern "C" fn(Ptr, Ptr),
    initialize: unsafe extern "C" fn(Ptr, Ptr),
    update: unsafe extern "C" fn(Ptr, Ptr),
    finish: unsafe extern "C" fn(Ptr),
}

/// `AtDriverNodeMethods`.
#[repr(C)]
pub struct DriverMethods {
    supports_pixel_type: unsafe extern "C" fn(Ptr, u8) -> bool,
    extension: unsafe extern "C" fn() -> *const *const c_char,
    open: unsafe extern "C" fn(Ptr, Ptr, BBox2, BBox2, c_int),
    needs_bucket: unsafe extern "C" fn(Ptr, c_int, c_int, c_int, c_int, u16) -> bool,
    prepare_bucket: unsafe extern "C" fn(Ptr, c_int, c_int, c_int, c_int, u16),
    process_bucket: unsafe extern "C" fn(Ptr, Ptr, Ptr, c_int, c_int, c_int, c_int, u16),
    write_bucket: unsafe extern "C" fn(Ptr, Ptr, Ptr, c_int, c_int, c_int, c_int),
    close: unsafe extern "C" fn(Ptr, Ptr),
}

/// `AtNodeMethods`.
#[repr(C)]
pub struct NodeMethods {
    common: *const CommonMethods,
    driver: *const DriverMethods,
}

// SAFETY: pointers to statics, never written.
unsafe impl Sync for NodeMethods {}

static COMMON: CommonMethods = CommonMethods {
    plugin_initialize,
    plugin_cleanup,
    parameters,
    initialize,
    update,
    finish,
};

static DRIVER: DriverMethods = DriverMethods {
    supports_pixel_type,
    extension,
    open,
    needs_bucket,
    prepare_bucket,
    process_bucket,
    write_bucket,
    close,
};

pub static METHODS: NodeMethods = NodeMethods {
    common: &COMMON,
    driver: &DRIVER,
};

/// Ready to be installed: what it fills, and the calls it makes.
pub fn prepare(pixels: Arc<Pixels>, calls: Calls) {
    if let Ok(mut p) = PIXELS.lock() {
        *p = Some(pixels);
    }
    let _ = CALLS.set(calls);
}

fn pixels() -> Option<Arc<Pixels>> {
    PIXELS.lock().ok()?.clone()
}

unsafe extern "C" fn plugin_initialize(_: *mut Ptr) -> bool {
    true
}
unsafe extern "C" fn plugin_cleanup(_: Ptr) {}
/// No parameters of its own.
unsafe extern "C" fn parameters(_: Ptr, _: Ptr) {}
unsafe extern "C" fn initialize(_: Ptr, node: Ptr) {
    if let Some(c) = CALLS.get() {
        // SAFETY: Arnold's call, on the node it is initialising; one output.
        unsafe { (c.driver_initialize)(node, false) };
    }
}
unsafe extern "C" fn update(_: Ptr, _: Ptr) {}
unsafe extern "C" fn finish(_: Ptr) {}

unsafe extern "C" fn supports_pixel_type(_: Ptr, ty: u8) -> bool {
    matches!(ty, ffi::TYPE_RGBA | ffi::TYPE_RGB)
}

/// A display: no file extension.
unsafe extern "C" fn extension() -> *const *const c_char {
    struct Extensions([*const c_char; 1]);
    // SAFETY: a list of no extensions, never written.
    unsafe impl Sync for Extensions {}
    static NONE: Extensions = Extensions([std::ptr::null()]);
    NONE.0.as_ptr()
}

unsafe extern "C" fn open(_: Ptr, _: Ptr, display: BBox2, _data: BBox2, _: c_int) {
    if let Some(p) = pixels() {
        let (w, h) = (display.maxx - display.minx + 1, display.maxy - display.miny + 1);
        p.open(display.minx, display.miny, w.max(0) as usize, h.max(0) as usize);
    }
}

unsafe extern "C" fn needs_bucket(_: Ptr, _: c_int, _: c_int, _: c_int, _: c_int, _: u16) -> bool {
    true
}
unsafe extern "C" fn prepare_bucket(_: Ptr, _: c_int, _: c_int, _: c_int, _: c_int, _: u16) {}

/// A bucket done, on the render thread that did it: its first output's pixels
/// into the image.
unsafe extern "C" fn process_bucket(_: Ptr, outputs: Ptr, _: Ptr, x: c_int, y: c_int, w: c_int, h: c_int, _: u16) {
    let (Some(p), Some(c)) = (pixels(), CALLS.get()) else {
        return;
    };
    let mut name = AtString::default();
    let mut ty: c_int = 0;
    let mut data: *const c_void = std::ptr::null();
    // SAFETY: Arnold's iterator over this bucket's outputs.
    if !unsafe { (c.output_next)(outputs, &mut name, &mut ty, &mut data) } || data.is_null() {
        return;
    }
    let (w, h) = (w.max(0) as usize, h.max(0) as usize);
    // SAFETY: a bucket of `w * h` pixels of the type said, Arnold's to keep.
    unsafe {
        match ty as u8 {
            ffi::TYPE_RGBA => {
                let px = std::slice::from_raw_parts(data as *const [f32; 4], w * h);
                p.bucket(x, y, w, h, |i| px[i]);
            }
            ffi::TYPE_RGB => {
                let px = std::slice::from_raw_parts(data as *const [f32; 3], w * h);
                p.bucket(x, y, w, h, |i| [px[i][0], px[i][1], px[i][2], 1.0]);
            }
            _ => {}
        }
    }
}

/// Done in `process_bucket`, on the render threads.
unsafe extern "C" fn write_bucket(_: Ptr, _: Ptr, _: Ptr, _: c_int, _: c_int, _: c_int, _: c_int) {}
unsafe extern "C" fn close(_: Ptr, _: Ptr) {}
