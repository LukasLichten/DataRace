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
pub fn create_property <S: ToString>(handle: &PluginHandle, name: S, init: Property) -> DataStoreReturnCode {
    let name_ptr = create_cstring!(name);

    let res = unsafe {
        sys::create_property(handle.get_ptr(), name_ptr, init.to_c())
    };
    drop_cstring!(name_ptr);


    DataStoreReturnCode::from(res)
}

/// Updates the value of a property
/// 
/// You can only update propertys that were created with this handle
/// You can only use values of the same type as the inital type, call change_property_type to cahnge this
pub fn update_property(handle: &PluginHandle, prop_handle: &PropertyHandle, value: Property) -> DataStoreReturnCode {
    let res = unsafe {
        sys::update_property(handle.get_ptr(), prop_handle.get_inner(), value.to_c())
    };

    DataStoreReturnCode::from(res)
}

/// Retrieves the value for a certain PropertyHandle
/// 
/// It is better to subscribe to a property, as this function incurrs a certain overhead (especially for string).
/// But if you rarely need this value, then the overhead from polling this value might be worth it
pub fn get_property_value(handle: &PluginHandle, prop_handle: &PropertyHandle) -> Result<Property, DataStoreReturnCode> {
    let res = unsafe {
        sys::get_property_value(handle.get_ptr(), prop_handle.get_inner())
    };

    let code = DataStoreReturnCode::from(res.code);
    if code != DataStoreReturnCode::Ok {
        return Err(code);
    }

    Ok(Property::new(res.value))
}

/// Retrieves the PropertyHandle used for reading and updating values
/// 
/// It is highly adviced you store this value, as retrieving a new handle for every api call is very expensive
/// PropertyHandles can become invalid (if for example a property gets renamed or deleted), then a new one has to be requested
pub fn generate_property_handle<S: ToString>(handle: &PluginHandle, name: S) -> Result<PropertyHandle, DataStoreReturnCode> {
    let name_ptr = create_cstring!(name);

    let res = unsafe {
        sys::generate_property_handle(handle.get_ptr(), name_ptr)
    };
    drop_cstring!(name_ptr);

    
    let code = DataStoreReturnCode::from(res.code);
    if code != DataStoreReturnCode::Ok {
        return Err(code);
    }

    Ok(PropertyHandle::new(res.value))
}

/// Deletes this property
pub fn delete_property(handle: &PluginHandle, prop_handle: PropertyHandle) -> DataStoreReturnCode {
    let res = unsafe {
        sys::delete_property(handle.get_ptr(), prop_handle.get_inner())
    };

    DataStoreReturnCode::from(res)
}

/// Subscribes you to a property, this will allow you to receive messages whenever this value
/// changes (sort of). Values are gathered leveraging the async runtime, so this is preferable over
/// polling manually via get_property_value.
///
/// If the type is a string you will receive a message for each time the value is updated
/// However all other types are polled by the pluginmanager, with messages send when at least one
/// changed. This means there is no guarantee that you will see all values.
/// Polling manually does not garantee this either
pub fn subscribe_property(handle: &PluginHandle, prop_handle: &PropertyHandle) -> DataStoreReturnCode {
    let res = unsafe {
        sys::subscribe_property(handle.get_ptr(), prop_handle.get_inner())
    };

    DataStoreReturnCode::from(res)
}

/// Removes subscription off this plugin from a certain property
///
/// You may after this call still receive some messages from updates of this property for a brief
/// time as the message queue is emptied
pub fn unsubscribe_property(handle: &PluginHandle, prop_handle: &PropertyHandle) -> DataStoreReturnCode {
    let res = unsafe {
        sys::unsubscribe_property(handle.get_ptr(), prop_handle.get_inner())
    };

    DataStoreReturnCode::from(res)
}

// TODO reenqueue message function... although not really necessary
