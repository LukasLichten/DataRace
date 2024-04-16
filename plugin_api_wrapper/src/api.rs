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


/// Creates a new Property (or more like queues it's creation)
///
/// The Property will not be immediatly created, it is only checked if the prop_handle is correct.
/// A message is instead send to the Pluginloader task for this plugin, which will then lock this
/// plugin, add the property and unlock.
/// Keep in mind, due to message backlog, there is no garantee it is added on the next unlock
/// cycle.
///
/// The name of the property in the end will get the name of the plugin prefixed:
/// plugin_name.name
/// The initial value will determine the Type of this Property, as long as you don't call
/// change_property_type it will be only possible to update using the same type
pub fn create_property <S: ToString>(handle: &PluginHandle, name: S, prop_handle: &PropertyHandle, init: Property) -> DataStoreReturnCode {
    let name_ptr = create_cstring!(name);

    let res = unsafe {
        sys::create_property(handle.get_ptr(), name_ptr, prop_handle.get_inner(), init.to_c())
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

/// Retrieves the value for a PropertyHandle that you have subscribe to (or created)
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

/// Generates the PropertyHandle used for reading and updating values
/// 
/// Preferrably you use the `crate::macros::generate_property_handle!()` macro to generate this
/// handle at compiletime, which allows you to cut down on overhead.
/// But in case of dynmaics where the name of the property could change this function is better,
/// but still, it is highly adviced you store this value
///
/// Property names are not case sensitive, have to contain at least one dot, with the first dot
/// deliminating between plugin and property (but the property part can contain further dots).
/// You can not have any leading or trailing dots
pub fn generate_property_handle<S: ToString>(name: S) -> Result<PropertyHandle, DataStoreReturnCode> {
    let name_ptr = create_cstring!(name);

    let res = unsafe {
        sys::generate_property_handle(name_ptr)
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
