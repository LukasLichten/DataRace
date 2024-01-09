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

/// Creates a new property, and returns (if it succeeds) the PropertyHandle of this Property
///
/// Keep in mind, the name of your plugin will be prepended to the final name: plugin_name.name
/// Also the initial value set the datatype, you can only use this type when calling update 
/// you need to call change_property_type to change this type
#[no_mangle]
pub extern "C" fn create_property(handle: *mut PluginHandle, name: *mut c_char, value: Property) -> ReturnValue<PropertyHandle> {
    let han = get_handle_val!(handle);
    let msg = get_string!(name);

    // This is shitty, but this is hopefully a decent stopgap
    let res = futures::executor::block_on(async {
        let mut ds = han.datastore.write().await;
        ds.create_property(&han.token, msg, utils::Value::new(value)).await
    });
    
    
    ReturnValue::from(res) 
}

/// Updates the value for the Property behind a given handle
/// 
/// You can only use values of the same type as the inital value
/// This method can NOT change the type, call change_property_type for this
#[no_mangle]
pub extern  "C" fn update_property(handle: *mut PluginHandle, prop_handle: PropertyHandle, value: Property) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);
    
    let res = futures::executor::block_on(async {
        let ds = han.datastore.read().await;
        ds.update_property(&han.token, &prop_handle, utils::Value::new(value)).await
    });

    res
}

/// Returns the value for a given property handle
/// 
/// This function is not as performant as subscribing to the property (especially for Strings),
/// but if you rarely poll this value then the overhead from this function is likely small enough
#[no_mangle]
pub extern "C" fn get_property_value(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> ReturnValue<Property> {
    let han = get_handle_val!(handle);

    let res = futures::executor::block_on(async {
        let ds = han.datastore.read().await;
        ds.get_property(&prop_handle).await
    });

    ReturnValue::from(match res {
        Ok(val) => {
            Property::try_from(val)
        },
        Err(e) => Err(e)
    })
}

/// Retrieves the PropertyHandle for a certain name
/// 
/// Similar to create_property, it is your job to deallocate the nullterminating string
/// It is adviced you store this PropertyHandle to avoid the penalty from having to request a new one for every API call
/// PropertyHandles can become outdated (when a property is renamed or deleted), then a new one has to be requested
#[no_mangle]
pub extern "C" fn get_property_handle(handle: *mut PluginHandle, name: *mut c_char) -> ReturnValue<PropertyHandle> {
    let han = get_handle_val!(handle);
    let msg = get_string!(name);

    let res = futures::executor::block_on(async {
        let ds = han.datastore.read().await;
        ds.get_property_handle(msg)
    });

    ReturnValue::from(res)
}

/// Deletes a certain property based on the Handle
#[no_mangle]
pub extern "C" fn delete_property(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);

    let res = futures::executor::block_on(async {
        let mut ds = han.datastore.write().await;
        ds.delete_property(&han.token, &prop_handle).await
    });
    res
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
