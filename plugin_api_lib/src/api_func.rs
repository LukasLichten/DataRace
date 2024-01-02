use libc::c_char;
use log::error;

use crate::{utils,PluginHandle,DataStoreReturnCode,PropertyHandle,Property,ReturnValue};


macro_rules! get_handle {
    ($ptr:ident) => {
        if let Some(handle) = unsafe {
            $ptr.as_ref()
        } {
            handle
        } else {
            error!("Plugin Handle corrupted");
            return;
        }
    };
    ($ptr:ident, $re: expr) => {
        if let Some(handle) = unsafe {
            $ptr.as_ref()
        } {
            handle
        } else {
            error!("Plugin Handle corrupted");
            return $re;
        }
    };
}

macro_rules! get_handle_val {
    ($ptr:ident) => {
        if let Some(handle) = unsafe {
            $ptr.as_ref()
        } {
            handle
        } else {
            error!("Plugin Handle corrupted");
            return ReturnValue::from(Err(DataStoreReturnCode::DataCorrupted));
        }
    };
}

macro_rules! get_string {
    ($ptr:ident) => {
        if let Some(msg) = utils::get_string($ptr) {
            msg
        } else {
            error!("Passed in String Corrupt");
            return ReturnValue::from(Err(DataStoreReturnCode::DataCorrupted));
        }
    };
}

#[no_mangle]
pub extern "C" fn create_property(handle: *mut PluginHandle, name: *mut c_char, value: Property) -> ReturnValue<PropertyHandle> {
    let han = get_handle_val!(handle);
    let msg = get_string!(name);

    // This is shitty, but this is hopefully a decent stopgap
    let res = futures::executor::block_on(han.datastore.create_property(&han.token, msg, utils::Value::new(value)));
    
    
    ReturnValue::from(res) 
}

/// Logs a null terminated String as a Info
/// String is not deallocated, that is your job
#[no_mangle]
pub extern "C" fn log_info(handle: *mut PluginHandle, message: *mut c_char) {
    log_plugin_msg(handle, message, log::Level::Info);
}

/// Logs a null terminated String as a Error
/// String is not deallocated, that is your job
#[no_mangle]
pub extern "C" fn log_error(handle: *mut PluginHandle, message: *mut c_char) {
    log_plugin_msg(handle, message, log::Level::Error);
}

fn log_plugin_msg(handle: *mut PluginHandle, message: *mut c_char, log_level: log::Level) {
    let han = get_handle!(handle); 

    let msg = if let Some(message) = utils::get_string(message) {
        message
    } else {
        error!("Message was corrupted");
        return;
    };

    // Even with file and or module set, it will continue not logging the name we want
    // So this is the best bandage fix over this mess
    log::logger().log(&log::Record::builder()
        .level(log_level)
        .args(format_args!("[{}] {msg}", han.name))
        .build());
}
