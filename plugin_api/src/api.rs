use std::{ffi::CString, os::raw::c_void};
use crate::wrappers::{Action, ActionHandle, DataStoreReturnCode, EventHandle, PluginHandle, PluginLockGuard, Property, PropertyHandle};

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

impl PluginHandle {
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
    /// You can only use values of the same type as the inital type, call `change_property_type` to change this.
    ///
    /// Additionally Array types can not be updated through this function,
    /// for regular updates use `get_property_value` to retireve the handle and then update using the handle,
    /// for resizing/retyping use `change_property_type` with a new Array too
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

    /// Creates a new Event (if it doesn't exists already).
    ///
    /// This is done by sending a message to the event loop, so we don't know if the event already
    /// exists, and it may take time to be created.
    /// Also you can only create events from your plugin.
    ///
    /// But as all Event related calls go through the event loop it is guaranteed that the event
    /// exists for any trigger calls following this function
    pub fn create_event(&self, event_handle: EventHandle) -> DataStoreReturnCode {
        let res = unsafe {
            sys::create_event(self.get_ptr(), event_handle.get_inner())
        };

        DataStoreReturnCode::from(res)
    }

    /// Deletes a Event.
    ///
    /// This is done by sending a message to the event loop, so we don't know if the event even
    /// existed, and it may take time to execute.
    /// Also you can only delete events from your plugin.
    ///
    /// But as all Event related calls go through the event loop it is guaranteed that the event
    /// will not exist for any event related calls after this function
    pub fn delete_event(&self, event_handle: EventHandle) -> DataStoreReturnCode {
        let res = unsafe {
            sys::delete_event(self.get_ptr(), event_handle.get_inner())
        };

        DataStoreReturnCode::from(res)
    }

    /// Subscribes to an event
    ///
    /// This is done by sending a message to the event loop, so we don't know if the event even
    /// exists, and it may take time to execute.
    ///
    /// If an event does not exist, then it will bookmark it, and automatically subscribe it once the
    /// plugin finally creates it.
    /// If that plugin is shut down before creation, then you are still notfied of unsubscription 
    /// (this is only for plugins shutdown after this function call, excluding plugin shutdown caused by datarace shutting down in general).
    ///
    /// It is possible that the first triggering of the event is already queued, then this subscription
    /// will miss the first trigger.
    pub fn subscribe_event(&self, event_handle: EventHandle) -> DataStoreReturnCode {
        let res = unsafe {
            sys::subscribe_event(self.get_ptr(), event_handle.get_inner())
        };

        DataStoreReturnCode::from(res)
    }

    /// Unsubscribes to an event
    ///
    /// This is done by sending a message to the event loop, so we don't know if the event even
    /// exists (or if we were even subscribed to it), and it may take time to execute.
    ///
    /// As such you may see some more events that where queued before this unsubscription. 
    ///
    /// You will be notified when the unsubscribe is complete, but only if the event existed (and you
    /// were subscribed).
    pub fn unsubscribe_event(&self, event_handle: EventHandle) -> DataStoreReturnCode {
        let res = unsafe {
            sys::unsubscribe_event(self.get_ptr(), event_handle.get_inner())
        };

        DataStoreReturnCode::from(res)
    }

    /// Triggers an event
    ///
    /// It sends a message to the event loop, so there is no confirmation that your event exists.
    ///
    /// While there can be delays before execution, the creation/deletion/other trigger calls are
    /// guaranteed to not be reordered (although this function call itself is not atomic, so
    /// parallel calls may cause inconsitencies)
    pub fn trigger_event(&self, event_handle: EventHandle) -> DataStoreReturnCode {
        let res = unsafe {
            sys::trigger_event(self.get_ptr(), event_handle.get_inner())
        };

        DataStoreReturnCode::from(res)
    }

    /// Triggers an Action
    ///
    /// An Action is an action (duh) performed by another plugin (you could technically also call
    /// this on yourself), given the optional parameters.
    ///
    /// If the action is successfully dispatched (requiring the other plugin to exist) you get a
    /// action_id back, which is unique to this invokation (with later invokations having usually a
    /// higher number).  
    /// You may receive a ActionCallbackRecv Event, which you can identify by this id.
    /// This event will also tell you if your call succeded, and may return parameters too.
    pub fn trigger_action(&self, action_handle: ActionHandle, params: Option<Vec<Property>>) -> Result<u64, DataStoreReturnCode> {
        let (params, param_count) = if let Some(params) = params {
            unsafe { crate::wrappers::vec_to_property_array(params) }
        } else {
            (std::ptr::null_mut(), 0)
        };

        let res = unsafe {
            sys::trigger_action(self.get_ptr(), action_handle.get_inner(), params, param_count)
        };

        let code = DataStoreReturnCode::from(res.code);
        if code != DataStoreReturnCode::Ok {
            return Err(code);
        }

        Ok(res.value)
    }


    /// Triggers the callback for an Action that was called on you.
    ///
    /// It is recommended to do a callback on every action you get, even if it is for an
    /// unsupported action.
    ///
    /// return_code of 0 singals success, any other code implies failure. What code you use for
    /// specific errors is up to you, but do document it for any users.
    ///
    /// You can pass back optional parameters (irrelevant if it is an error or not).
    ///
    /// You will get a Ok if the callback was successfully send (requires the other plugin to exist)
    pub fn action_callback(&self, action: Action, return_code: u64, params: Option<Vec<Property>>) -> DataStoreReturnCode {
        let (params, param_count) = if let Some(params) = params {
            unsafe { crate::wrappers::vec_to_property_array(params) }
        } else {
            (std::ptr::null_mut(), 0)
        };

        let res = unsafe {
            sys::action_callback(self.get_ptr(), action.to_c(), return_code, params, param_count)
        };

        DataStoreReturnCode::from(res)
    }

    /// Allows you to send a raw memory pointer to another plugin.  
    ///
    /// The target is plugin id of the target plugin.  
    /// reason serves as a way to communicate what this pointer is for, although the recipient is also
    /// told your plugin id.  
    /// Obviously managing void pointers is risky business, both recipients have to be on the same
    /// package and understand what it stands for.
    pub unsafe fn send_plugin_ptr_message(&self, target: u64, ptr: *mut c_void, reason: i64) -> DataStoreReturnCode {
        let res = unsafe {
            sys::send_ptr_msg_to_plugin(self.get_ptr(), target, ptr, reason)
        };

        DataStoreReturnCode::from(res)
    }

    /// Sends a message to the update function of your plugin.  
    /// This type of internal message is useful for sending messages from worker threads, for example
    /// that they failed, so you could restart them or shut the plugin down
    pub fn send_internal_msg(&self, msg: i64) -> DataStoreReturnCode {
        let res = unsafe {
            sys::send_internal_msg(self.get_ptr(), msg)
        };

        DataStoreReturnCode::from(res)
    }

    
    /// This is a way to Sync between your worker thread and the pluginloader.
    /// While you set the plugin to locked the pluginloader will not intiate lock,
    /// so you Don't need to provide your own sync mechanism through state and Lock/Unlock Messages.  
    /// This function is blocking, and uses atomic wait to send the thread into sleep while waiting.
    ///
    /// However, you will still receive Lock and Unlock Message, especially Lock Messages will come
    /// while your worker might still be holding the lock (as they come before the loader goes into
    /// waiting for lock).  
    /// 
    /// Also it is important to unlock the plugin periodically, so the pluginloader can do mutable
    /// work.
    /// Futher, DO NOT request a lock while holding another PluginLockGuard (in the same context), this will lead to a
    /// deadlock (as the first can't be dropped to unlock)!
    ///
    /// Once the Guard is dropped the plugin unlocks
    pub fn lock_plugin<'a>(&'a self) -> PluginLockGuard<'a> {
        unsafe { sys::lock_plugin(self.get_ptr()) };

        PluginLockGuard { handle: self }
    }
}

/// Generates the PropertyHandle used for reading and updating values.
/// 
/// Preferrably you use the `crate::macros::generate_property_handle!()` macro to generate this
/// handle at compiletime, which allows you to cut down on overhead.
/// But in case of dynmaics where the name of the property could change this function is better,
/// but still, it is highly adviced you store this value.
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

/// Generates the EventHandle used for creating, deleting, triggering and identifzing incoming
/// events.
/// 
/// Preferrably you use the `crate::macros::generate_event_handle!()` macro to generate this
/// handle at compiletime, which allows you to cut down on overhead.
/// But in case of dynmaics where the name of the event could change this function is better,
/// but still, it is highly adviced you store this value.
///
/// Event names are not case sensitive, have to contain at least one dot, with the first dot
/// deliminating between plugin and event (but the event part can contain further dots).
/// You can not have any leading or trailing dots
pub fn generate_event_handle<S: ToString>(name: S) -> Result<EventHandle, DataStoreReturnCode> {
    let name_ptr = create_cstring!(name);

    let res = unsafe {
        sys::generate_event_handle(name_ptr)
    };
    drop_cstring!(name_ptr);

    
    let code = DataStoreReturnCode::from(res.code);
    if code != DataStoreReturnCode::Ok {
        return Err(code);
    }

    Ok(EventHandle::new(res.value))
}

/// Generates the ActionHandle used for triggering Actions in other plugins
/// 
/// Preferrably you use the `crate::macros::generate_action_handle!()` macro to generate this
/// handle at compiletime, which allows you to cut down on overhead.
/// But in case of dynmaics where the name of the action could change this function is better,
/// but still, it is highly adviced you store this value.
///
/// Action names are not case sensitive, have to contain at least one dot, with the first dot
/// deliminating between plugin and action (but the action part can contain further dots).
/// You can not have any leading or trailing dots
pub fn generate_action_handle<S: ToString>(name: S) -> Result<ActionHandle, DataStoreReturnCode> {
    let name_ptr = create_cstring!(name);

    let res = unsafe {
        sys::generate_action_handle(name_ptr)
    };
    drop_cstring!(name_ptr);

    
    let code = DataStoreReturnCode::from(res.code);
    if code != DataStoreReturnCode::Ok {
        return Err(code);
    }

    Ok(ActionHandle::new(res.value))
}

/// Allows you to optain the id of another plugin based on it's name. 
/// This function is intended for runtime use, compiletime macro is TODO
///
/// This function also checks if the name does not contain any invalid characters (currently only .),
/// but does not check if the plugin is loaded.
pub fn generate_foreign_plugin_id<S: ToString>(handle: &PluginHandle, name: S) -> Option<u64> {
    let name_ptr = create_cstring!(name);

    let res = unsafe {
        sys::get_foreign_plugin_id(handle.get_ptr(), name_ptr)
    };
    drop_cstring!(name_ptr);

    if res.valid {
        Some(res.id)
    } else {
        None
    }
}

// TODO reenqueue message function... although not really necessary
