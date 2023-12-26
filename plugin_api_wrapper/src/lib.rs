use std::ffi::CString;


pub fn log_info(handle: *mut reexport::PluginHandle, msg: String) {
    let c_str = CString::new(msg).unwrap();
    
    unsafe {
        let ptr = c_str.into_raw();
        datarace_plugin_api_sys::log_info(handle, ptr);
        drop(CString::from_raw(ptr));
    }
}

pub mod reexport {
    pub use datarace_plugin_api_sys::PluginHandle;
}

pub mod macros {
    pub use wrapper_macro::*;
}
