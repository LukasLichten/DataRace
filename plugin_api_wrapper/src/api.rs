use std::ffi::CString;
use crate::wrappers::PluginHandle;

/// Logs a message with info level
pub fn log_info <S: ToString>(handle: PluginHandle, msg: S) {
    let c_str = CString::new(msg.to_string()).unwrap();
    
    unsafe {
        let ptr = c_str.into_raw();
        datarace_plugin_api_sys::log_info(handle.get_ptr(), ptr);
        drop(CString::from_raw(ptr));
    }
}
