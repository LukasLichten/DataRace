use std::ffi::CString;
use crate::wrappers::{PluginHandle, Property, PropertyHandle, DataStoreReturnCode};

use datarace_plugin_api_sys as sys;

macro_rules! create_cstring {
    ($msg: ident) => {
        CString::new($msg.to_string()).unwrap().into_raw()
    };
}
macro_rules! drop_cstring {
    ($ptr: ident) => {
        unsafe {
            drop(CString::from_raw($ptr));
        }
    };
}


/// Logs a message with info level
pub fn log_info <S: ToString>(handle: &PluginHandle, msg: S) {
    let ptr = create_cstring!(msg);
    
    unsafe {
        sys::log_info(handle.get_ptr(), ptr);
    }
    drop_cstring!(ptr)
}

/// Logs a message with error level
pub fn log_error <S: ToString>(handle: &PluginHandle, msg: S) {
    let ptr = create_cstring!(msg);

    unsafe {
        sys::log_error(handle.get_ptr(), ptr);
    }
    drop_cstring!(ptr);
}


/// Creates a new Property, and returns when successful the PropertyHandle
///
/// The name of the property in the end will get the name of the plugin prefixed:
/// plugin_name.name
/// The initial value will determine the Type of this Property, as long as you don't call
/// change_property_type it will be only possible to update using the same type
pub fn create_property <S: ToString>(handle: &PluginHandle, name: S, init: Property) -> Result<PropertyHandle,DataStoreReturnCode> {
    let name_ptr = create_cstring!(name);

    let res = unsafe {
        sys::create_property(handle.get_ptr(), name_ptr, init.to_c())
    };
    drop_cstring!(name_ptr);


    let code = DataStoreReturnCode::from(res.code);
    if code != DataStoreReturnCode::Ok {
        return Err(code);
    }

    Ok(PropertyHandle::new(res.value))
}
