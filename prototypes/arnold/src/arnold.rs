//! The functions of Arnold's API the prototype calls, looked up in `libai` at
//! run time: nothing is linked at build time. Declarations follow the Arnold 7
//! API reference; handles are opaque pointers.

use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};

pub type Ptr = *mut c_void;

/// Arnold's string: one pointer to text Arnold keeps (`AtString`).
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct AtString(*const c_char);

pub const SESSION_INTERACTIVE: c_int = 1;
pub const RENDER_MODE_CAMERA: c_int = 0;
pub const NODE_LIGHT: u32 = 0x0004;
pub const NODE_DRIVER: u32 = 0x0040;
pub const NODE_ALL: u32 = 0xFFFF;
/// Info, warnings, errors and progress (`AI_LOG_*`).
pub const LOG: c_int = 0x0001 | 0x0002 | 0x0004 | 0x0080;
/// `AtRenderStatus`, in its order.
pub const STATUS: [&str; 6] = ["not started", "paused", "restarting", "rendering", "finished", "failed"];
pub const RENDERING: c_int = 3;
pub const RESTARTING: c_int = 2;
pub const FINISHED: c_int = 4;
pub const FAILED: c_int = 5;
/// `AtRenderErrorCode`, in its order: why a render did not start.
pub const ERRORS: [&str; 9] = [
    "success",
    "aborted",
    "no camera: the layer has none, or its Render Settings name none",
    "a bad camera",
    "the scene did not validate",
    "a bad render region",
    "interrupted",
    "no outputs: the layer's Render Settings and Products give Arnold no image to make",
    "an error",
];

macro_rules! api {
    ($($name:ident: fn($($arg:ty),*) $(-> $ret:ty)?;)*) => {
        /// The functions, each found by its exported name.
        #[allow(non_snake_case)]
        pub struct Api {
            $(pub $name: unsafe extern "C" fn($($arg),*) $(-> $ret)?,)*
            _lib: Library,
        }

        impl Api {
            fn from(lib: Library) -> Result<Api, String> {
                // SAFETY: each symbol is the function of that name in libai,
                // its type as the API reference declares it.
                unsafe {
                    Ok(Api {
                        $($name: std::mem::transmute::<Ptr, unsafe extern "C" fn($($arg),*) $(-> $ret)?>(
                            lib.symbol(stringify!($name))?,
                        ),)*
                        _lib: lib,
                    })
                }
            }
        }
    };
}

api! {
    AiGetVersion: fn(*mut c_char, *mut c_char, *mut c_char, *mut c_char) -> *const c_char;
    AiBegin: fn(c_int);
    AiEnd: fn();
    AiCreateAtStringData_private: fn(*const c_char) -> *const c_char;
    AiUniverse: fn() -> Ptr;
    AiUniverseDestroy: fn(Ptr);
    AiUniverseGetOptions: fn(Ptr) -> Ptr;
    AiUniverseGetNodeIterator: fn(Ptr, u32) -> Ptr;
    AiNodeIteratorFinished: fn(Ptr) -> bool;
    AiNodeIteratorGetNext: fn(Ptr) -> Ptr;
    AiNodeIteratorDestroy: fn(Ptr);
    AiNodeLookUpByName: fn(Ptr, AtString, Ptr) -> Ptr;
    AiNodeGetName: fn(Ptr) -> *const c_char;
    AiNodeGetNodeEntry: fn(Ptr) -> Ptr;
    AiNodeEntryGetName: fn(Ptr) -> *const c_char;
    AiNodeGetFlt: fn(Ptr, AtString) -> f32;
    AiNodeGetBool: fn(Ptr, AtString) -> bool;
    AiNodeSetFlt: fn(Ptr, AtString, f32);
    AiNodeSetBool: fn(Ptr, AtString, bool);
    AiNodeGetStr: fn(Ptr, AtString) -> AtString;
    AiNodeSetStr: fn(Ptr, AtString, AtString);
    AiMsgSetConsoleFlags: fn(Ptr, c_int);
    AiParamValueMap: fn() -> Ptr;
    AiParamValueMapSetFlt: fn(Ptr, AtString, f32);
    AiParamValueMapDestroy: fn(Ptr);
    AiSceneLoad: fn(Ptr, *const c_char, Ptr) -> bool;
    AiRenderSession: fn(Ptr, c_int) -> Ptr;
    AiRenderSessionDestroy: fn(Ptr);
    AiRenderBegin: fn(Ptr, c_int, Ptr, Ptr) -> c_int;
    AiRenderInterrupt: fn(Ptr, c_int);
    AiRenderRestart: fn(Ptr);
    AiRenderGetStatus: fn(Ptr) -> c_int;
    AiRenderEnd: fn(Ptr) -> c_int;
}

impl Api {
    /// libai in the SDK at `sdk` (`$ARNOLD_PATH`): its `bin` folder.
    pub fn load(sdk: &Path) -> Result<Api, String> {
        let file = if cfg!(target_os = "macos") {
            "libai.dylib"
        } else if cfg!(windows) {
            "ai.dll"
        } else {
            "libai.so"
        };
        let path: PathBuf = sdk.join("bin").join(file);
        Api::from(Library::open(&path)?)
    }

    pub fn string(&self, s: &str) -> AtString {
        let c = CString::new(s).unwrap_or_default();
        // SAFETY: Arnold copies the text and keeps it for the process.
        AtString(unsafe { (self.AiCreateAtStringData_private)(c.as_ptr()) })
    }

    pub fn version(&self) -> String {
        // SAFETY: Arnold accepts null for the parts it is not asked for.
        let all = unsafe { (self.AiGetVersion)(null(), null(), null(), null()) };
        text(all)
    }

    /// Every node of `mask` in `universe`: its type and name.
    pub fn nodes(&self, universe: Ptr, mask: u32) -> Vec<(String, String, Ptr)> {
        let mut out = vec![];
        // SAFETY: the iterator is destroyed once walked.
        unsafe {
            let it = (self.AiUniverseGetNodeIterator)(universe, mask);
            while !(self.AiNodeIteratorFinished)(it) {
                let node = (self.AiNodeIteratorGetNext)(it);
                if node.is_null() {
                    continue;
                }
                let entry = (self.AiNodeGetNodeEntry)(node);
                out.push((
                    text((self.AiNodeEntryGetName)(entry)),
                    text((self.AiNodeGetName)(node)),
                    node,
                ));
            }
            (self.AiNodeIteratorDestroy)(it);
        }
        out
    }

    pub fn str_param(&self, node: Ptr, name: &str) -> String {
        // SAFETY: `node` is a live node; an unknown parameter gives "".
        text(unsafe { (self.AiNodeGetStr)(node, self.string(name)) }.0)
    }
}

fn null() -> *mut c_char {
    std::ptr::null_mut()
}

/// Text Arnold owns, copied.
pub fn text(p: *const c_char) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: Arnold's strings end with a nul.
    unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
}

/// A library opened by the system's loader, kept open while its functions are
/// used.
struct Library(Ptr);

#[cfg(unix)]
mod sys {
    use super::Ptr;
    use std::ffi::{c_char, c_int};
    #[cfg_attr(target_os = "linux", link(name = "dl"))]
    extern "C" {
        pub fn dlopen(file: *const c_char, mode: c_int) -> Ptr;
        pub fn dlsym(lib: Ptr, name: *const c_char) -> Ptr;
        pub fn dlerror() -> *const c_char;
    }
    /// RTLD_NOW | RTLD_GLOBAL: its plugins (the USD reader) find its symbols.
    #[cfg(target_os = "macos")]
    pub const MODE: c_int = 0x2 | 0x8;
    #[cfg(not(target_os = "macos"))]
    pub const MODE: c_int = 0x2 | 0x100;
}

#[cfg(windows)]
mod sys {
    use super::Ptr;
    use std::ffi::c_char;
    #[link(name = "kernel32")]
    extern "system" {
        pub fn LoadLibraryW(file: *const u16) -> Ptr;
        pub fn GetProcAddress(lib: Ptr, name: *const c_char) -> Ptr;
    }
}

impl Library {
    fn open(path: &Path) -> Result<Library, String> {
        if !path.exists() {
            return Err(format!("no Arnold library at {}", path.display()));
        }
        #[cfg(unix)]
        let lib = {
            let c = CString::new(path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
            // SAFETY: a path, opened by the system's loader.
            unsafe { sys::dlopen(c.as_ptr(), sys::MODE) }
        };
        #[cfg(windows)]
        let lib = {
            use std::os::windows::ffi::OsStrExt;
            let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
            // SAFETY: a nul-ended path.
            unsafe { sys::LoadLibraryW(wide.as_ptr()) }
        };
        if lib.is_null() {
            return Err(format!("{} would not load: {}", path.display(), loader_error()));
        }
        Ok(Library(lib))
    }

    fn symbol(&self, name: &str) -> Result<Ptr, String> {
        let c = CString::new(name).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        // SAFETY: a library this process opened.
        let f = unsafe { sys::dlsym(self.0, c.as_ptr()) };
        #[cfg(windows)]
        // SAFETY: a library this process opened.
        let f = unsafe { sys::GetProcAddress(self.0, c.as_ptr()) };
        if f.is_null() {
            return Err(format!("libai has no {name}: an Arnold older than 7?"));
        }
        Ok(f)
    }
}

fn loader_error() -> String {
    #[cfg(unix)]
    // SAFETY: the loader's last message, if any.
    return text(unsafe { sys::dlerror() });
    #[cfg(windows)]
    return std::io::Error::last_os_error().to_string();
}
