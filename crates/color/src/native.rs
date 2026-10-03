//! All unsafe operations are confined to this private, version-checked C ABI.
//! Handles own the loaded library; no borrowed native string escapes a call.
#![allow(unsafe_code)]
use libloading::Library;
use std::{
    ffi::{CStr, CString, c_char, c_int, c_long, c_void},
    path::Path,
    ptr::NonNull,
    rc::Rc,
};

type Handle = *mut c_void;
type DropHandle = unsafe extern "C" fn(Handle);
type ConfigFn = unsafe extern "C" fn(*const c_char, *mut c_char, usize) -> Handle;
type CountFn = unsafe extern "C" fn(Handle, c_int, *const c_char) -> c_int;
type NameFn = unsafe extern "C" fn(Handle, c_int, *const c_char, c_int) -> *const c_char;
type ProcessorFn = unsafe extern "C" fn(
    Handle,
    *const c_char,
    *const c_char,
    *const c_char,
    *const c_char,
    *mut c_char,
    usize,
) -> Handle;
type IdFn = unsafe extern "C" fn(Handle) -> *const c_char;
type FilesFn = unsafe extern "C" fn(Handle) -> c_int;
type FileFn = unsafe extern "C" fn(Handle, Handle, c_int, *mut c_char, usize) -> c_int;
type ApplyFn = unsafe extern "C" fn(Handle, *mut f32, c_long, *mut c_char, usize) -> c_int;

pub(crate) struct Api {
    _library: Library,
    config: ConfigFn,
    config_drop: DropHandle,
    count: CountFn,
    name: NameFn,
    processor: ProcessorFn,
    processor_drop: DropHandle,
    id: IdFn,
    files: FilesFn,
    file: FileFn,
    apply: ApplyFn,
}

fn text(s: &str) -> Result<CString, String> {
    CString::new(s).map_err(|_| "OCIO name/path contains NUL".into())
}
fn error(buffer: &[c_char]) -> String {
    // The C adapter always terminates caller-owned error buffers.
    unsafe { CStr::from_ptr(buffer.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}
fn owned(ptr: *const c_char) -> Result<String, String> {
    if ptr.is_null() {
        return Err("OCIO returned no string".into());
    }
    // Valid native handles retain the storage throughout this immediate copy.
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(str::to_owned)
        .map_err(|e| e.to_string())
}
impl Api {
    pub(crate) fn load(path: &Path) -> Result<Rc<Self>, String> {
        if !path.is_absolute() {
            return Err("OCIO library path must be absolute".into());
        }
        // Only load the explicit installation's private bridge, never search CWD.
        unsafe {
            let lib = Library::new(path).map_err(|e| format!("OCIO runtime: {e}"))?;
            let abi = lib
                .get::<unsafe extern "C" fn() -> u32>(b"fold_ocio_abi\0")
                .map_err(|e| e.to_string())?;
            let version = lib
                .get::<unsafe extern "C" fn() -> *const c_char>(b"fold_ocio_version\0")
                .map_err(|e| e.to_string())?;
            if abi() != 1 || owned(version())? != crate::OCIO_VERSION {
                return Err("incompatible OCIO runtime; expected ABI 1 / OCIO 2.4.2".into());
            }
            macro_rules! symbol {
                ($name:literal) => {
                    *lib.get(concat!($name, "\0").as_bytes())
                        .map_err(|e| e.to_string())?
                };
            }
            Ok(Rc::new(Self {
                config: symbol!("fold_ocio_config"),
                config_drop: symbol!("fold_ocio_config_drop"),
                count: symbol!("fold_ocio_count"),
                name: symbol!("fold_ocio_name"),
                processor: symbol!("fold_ocio_processor"),
                processor_drop: symbol!("fold_ocio_processor_drop"),
                id: symbol!("fold_ocio_processor_id"),
                files: symbol!("fold_ocio_files"),
                file: symbol!("fold_ocio_file"),
                apply: symbol!("fold_ocio_apply"),
                _library: lib,
            }))
        }
    }
}

pub(crate) struct Config {
    api: Rc<Api>,
    handle: NonNull<c_void>,
}
impl Config {
    pub(crate) fn load(api: Rc<Api>, path: &Path) -> Result<Self, String> {
        let path = text(path.to_str().ok_or("OCIO path must be UTF-8")?)?;
        let mut err = [0; 4096];
        let ptr = unsafe { (api.config)(path.as_ptr(), err.as_mut_ptr(), err.len()) };
        let handle = NonNull::new(ptr).ok_or_else(|| error(&err))?;
        Ok(Self { api, handle })
    }
    pub(crate) fn names(&self, kind: i32, display: &str) -> Result<Vec<String>, String> {
        let display = text(display)?;
        let count = unsafe { (self.api.count)(self.handle.as_ptr(), kind, display.as_ptr()) };
        if !(0..=16384).contains(&count) {
            return Err("invalid OCIO catalog size".into());
        }
        (0..count)
            .map(|index| {
                owned(unsafe {
                    (self.api.name)(self.handle.as_ptr(), kind, display.as_ptr(), index)
                })
            })
            .collect()
    }
    pub(crate) fn processor(
        &self,
        source: &str,
        destination: &str,
        view: &str,
        look: &str,
    ) -> Result<Processor, String> {
        let (source, destination, view, look) =
            (text(source)?, text(destination)?, text(view)?, text(look)?);
        let mut err = [0; 4096];
        let ptr = unsafe {
            (self.api.processor)(
                self.handle.as_ptr(),
                source.as_ptr(),
                destination.as_ptr(),
                view.as_ptr(),
                look.as_ptr(),
                err.as_mut_ptr(),
                err.len(),
            )
        };
        let handle = NonNull::new(ptr).ok_or_else(|| error(&err))?;
        Ok(Processor {
            api: self.api.clone(),
            handle,
        })
    }
    pub(crate) fn files(&self, processor: &Processor) -> Result<Vec<String>, String> {
        let count = unsafe { (self.api.files)(processor.handle.as_ptr()) };
        if !(0..=16384).contains(&count) {
            return Err("invalid OCIO file count".into());
        }
        (0..count)
            .map(|index| {
                let mut out = [0; 4096];
                let ok = unsafe {
                    (self.api.file)(
                        self.handle.as_ptr(),
                        processor.handle.as_ptr(),
                        index,
                        out.as_mut_ptr(),
                        out.len(),
                    )
                };
                if ok == 0 {
                    Err(error(&out))
                } else {
                    Ok(error(&out))
                }
            })
            .collect()
    }
}
impl Drop for Config {
    fn drop(&mut self) {
        unsafe { (self.api.config_drop)(self.handle.as_ptr()) };
    }
}
pub(crate) struct Processor {
    api: Rc<Api>,
    handle: NonNull<c_void>,
}
impl Processor {
    pub(crate) fn gpu(&self) -> Result<crate::GpuShader, String> {
        type Extract = unsafe extern "C" fn(Handle, *mut c_char, usize) -> Handle;
        // Optional ABI extension: existing CPU packages remain usable, while
        // GPU selection reports an actionable error for an older bridge.
        unsafe {
            let extract = self
                .api
                ._library
                .get::<Extract>(b"fold_ocio_gpu\0")
                .map_err(|_| "OCIO runtime lacks GPU support; rebuild the private color package")?;
            let json = self
                .api
                ._library
                .get::<IdFn>(b"fold_ocio_gpu_json\0")
                .map_err(|e| e.to_string())?;
            let drop = self
                .api
                ._library
                .get::<DropHandle>(b"fold_ocio_gpu_drop\0")
                .map_err(|e| e.to_string())?;
            let mut err = [0; 4096];
            let handle = extract(self.handle.as_ptr(), err.as_mut_ptr(), err.len());
            if handle.is_null() {
                return Err(error(&err));
            }
            let text = owned(json(handle));
            drop(handle);
            serde_json::from_str(&text?).map_err(|e| e.to_string())
        }
    }
    pub(crate) fn id(&self) -> Result<String, String> {
        owned(unsafe { (self.api.id)(self.handle.as_ptr()) })
    }
    pub(crate) fn apply(&self, pixels: &mut [[f32; 4]]) -> Result<(), String> {
        if pixels.is_empty() {
            return Ok(());
        }
        let count = c_long::try_from(pixels.len()).map_err(|_| "OCIO image too large")?;
        let mut err = [0; 4096];
        // Arrays are contiguous, RGBA32F, uniquely borrowed for the entire native call.
        let ok = unsafe {
            (self.api.apply)(
                self.handle.as_ptr(),
                pixels.as_mut_ptr().cast(),
                count,
                err.as_mut_ptr(),
                err.len(),
            )
        };
        if ok == 0 { Err(error(&err)) } else { Ok(()) }
    }
}
impl Drop for Processor {
    fn drop(&mut self) {
        unsafe { (self.api.processor_drop)(self.handle.as_ptr()) };
    }
}
