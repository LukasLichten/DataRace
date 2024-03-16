/// Contains all functions of the api
pub mod api;

/// Contains wrappers around api data
pub mod wrappers;

/// Serves to reexport certain C structs for purposes such as building callback functions
pub mod reexport {
    pub use datarace_plugin_api_sys::PluginHandle;
    pub use datarace_plugin_api_sys::Message;
    pub use datarace_plugin_api_sys::PluginDescription;
}

/// For building callback functions simply
pub mod macros {
    pub use wrapper_macro::*;
}

use std::ffi::CStr;

/// Simple way to aquire a String for a null terminating c_char ptr
/// We do not optain ownership of the String, the owner has to deallocate it
pub fn get_string(ptr: *mut std::os::raw::c_char) -> Option<String> {
    Some(unsafe {
        let c_str = CStr::from_ptr(ptr);

        if let Ok(it) = c_str.to_str() {
            it
        } else {
            return None;
        }
    }.to_string())
}
