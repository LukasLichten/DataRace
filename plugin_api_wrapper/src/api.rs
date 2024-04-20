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

impl<T> PluginHandle<T> {
    /// Logs a message with info level
    pub fn log_info <S: ToString>(&self, msg: S) {
        let ptr = create_cstring!(msg);
        
        unsafe {
            sys::log_info(self.get_ptr(), ptr);
        }
        drop_cstring!(ptr)
    }

    /// Logs a message with error level
    pub fn log_error <S: ToString>(&self, msg: S) {
        let ptr = create_cstring!(msg);

        unsafe {
            sys::log_error(self.get_ptr(), ptr);
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
    pub fn create_property <S: ToString>(&self, name: S, prop_handle: PropertyHandle, init: Property) -> DataStoreReturnCode {
        let name_ptr = create_cstring!(name);

        let res = unsafe {
            sys::create_property(self.get_ptr(), name_ptr, prop_handle.get_inner(), init.to_c())
        };
        drop_cstring!(name_ptr);


        DataStoreReturnCode::from(res)
    }

    /// Updates the value of a property
    /// 
    /// You can only update propertys that were created with this handle
    /// You can only use values of the same type as the inital type, call change_property_type to cahnge this
    pub fn update_property(&self, prop_handle: PropertyHandle, value: Property) -> DataStoreReturnCode {
        let res = unsafe {
            sys::update_property(self.get_ptr(), prop_handle.get_inner(), value.to_c())
        };

        DataStoreReturnCode::from(res)
    }

    /// Retrieves the value for a PropertyHandle that you have subscribe to (or created)
    pub fn get_property_value(&self, prop_handle: PropertyHandle) -> Result<Property, DataStoreReturnCode> {
        let res = unsafe {
            sys::get_property_value(self.get_ptr(), prop_handle.get_inner())
        };

        let code = DataStoreReturnCode::from(res.code);
        if code != DataStoreReturnCode::Ok {
            return Err(code);
        }

        Ok(Property::new(res.value))
    }


    /// Deletes this property (queues the deletion)
    ///
    /// Same as create, this (after checking that the property exists) will the send a message to
    /// the loader which locks the plugin to perform the delete. The queue length is unknown,
    /// so it can take multiple locks and unlocks till this action is performed
    ///
    /// You can only delete Properties you created
    pub fn delete_property(&self, prop_handle: PropertyHandle) -> DataStoreReturnCode {
        let res = unsafe {
            sys::delete_property(self.get_ptr(), prop_handle.get_inner())
        };

        DataStoreReturnCode::from(res)
    }

    /// Changes the type of this property (or more like queues this change)
    ///
    /// Same as create and delete, this (after checking that the property exists) will the send a message to
    /// the loader which locks the plugin to perform the delete. The queue length is unknown,
    /// so it can take multiple locks and unlocks till this action is performed
    ///
    /// You can only change type of Properties you created
    pub fn change_property_type(&self, prop_handle: PropertyHandle, value: Property) -> DataStoreReturnCode {
        let res = unsafe {
            sys::change_property_type(self.get_ptr(), prop_handle.get_inner(), value.to_c())
        };

        DataStoreReturnCode::from(res)
    }

    /// Subscribes you to a property (or more like queues the action)
    /// After this finishes you can access this property through get_property_value
    ///
    /// Similar to create/delete/change_type, this queues the subscribe action.
    /// However, in this case do not know if the property we are trying to add exists, as we send a
    /// message to our pluginloader, which will then look up and send a message to loader of the plugin
    /// for this property, then this respondes back to our loader, which will then add it to the
    /// subscriptions (for which it will lock)
    pub fn subscribe_property(&self, prop_handle: PropertyHandle) -> DataStoreReturnCode {
        let res = unsafe {
            sys::subscribe_property(self.get_ptr(), prop_handle.get_inner())
        };

        DataStoreReturnCode::from(res)
    }

    /// Removes subscription for a certain property (it will queue it)
    ///
    /// Same as create/change_property/delete, this (after checking that the property was subscribed to) will send a Message to the loader
    /// which locks the plugin to perform the removal. The queue length is unknown, so it can take
    /// multiple locks and unlocks till this action is performed
    pub fn unsubscribe_property(&self, prop_handle: PropertyHandle) -> DataStoreReturnCode {
        let res = unsafe {
            sys::unsubscribe_property(self.get_ptr(), prop_handle.get_inner())
        };

        DataStoreReturnCode::from(res)
    }
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

// TODO reenqueue message function... although not really necessary
