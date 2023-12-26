use libc::c_char;

#[allow(dead_code)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

#[cfg(feature = "main-entry")]
pub unsafe fn run() {
    bindings::run();
}

pub use bindings::PluginHandle;

pub unsafe fn log_info(handle: *mut PluginHandle, message: *mut c_char) {
    bindings::log_info(handle, message);    
}
