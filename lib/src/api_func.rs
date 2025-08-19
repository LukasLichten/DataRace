use std::sync::Arc;

use libc::{c_char, c_void};
use log::error;

use crate::{events::EventMessage, pluginloader::LoaderMessage, utils::{self, VoidPtrWrapper}, Action, ActionHandle, ArrayValueHandle, DataStoreReturnCode, EventHandle, Message, PluginDescription, PluginHandle, PluginNameHash, PluginSettingsLoadFail, PluginSettingsLoadReturn, PluginSettingsLoadState, Property, PropertyHandle, PropertyType, ReturnValue, API_VERSION};


macro_rules! get_handle {
    ($ptr:ident) => {
        if let Some(handle) = unsafe {
            $ptr.as_ref()
        } {
            handle
        } else {
            error!("PluginHandle can not be null");
            return;
        }
    };
    ($ptr:ident, $re: expr) => {
        if let Some(handle) = unsafe {
            $ptr.as_ref()
        } {
            handle
        } else {
            error!("PluginHandle can not be null");
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
            error!("PluginHandle can not be null");
            return ReturnValue::from(Err(DataStoreReturnCode::HandleNullPtr));
        }
    };
}

macro_rules! get_string {
    ($ptr:ident) => {
        if let Some(msg) = utils::get_string($ptr) {
            msg
        } else {
            error!("Passed in String Corrupt");
            return ReturnValue::from(Err(DataStoreReturnCode::ParameterCorrupted));
        }
    };
    ($ptr:ident, $re: expr) => {
        if let Some(msg) = utils::get_string($ptr) {
            msg
        } else {
            error!("Passed in String Corrupt");
            return $re;
        }
    };
}

/// Creates a new property (queues it for creation).
///
/// It will return errors if the property handle missmatches the name (and the plugin id missmaches
/// the current plugin name). But it won't detect id collisions.
/// In general, the property will not immediatly be created, instead sending it to the loader task,
/// which will through the update function lock the datastore to add it.
/// But you can't know how much of a backlog the channel going over the the pluginloader, so it
/// may take even longer.
/// But this is the safe way of adding properties, as it insures there is no race condition.
///
/// Keep in mind, the name of your plugin will be prepended to the final name: plugin_name.name
/// It is also your job to deallocate this name string.
/// Also the initial value set the datatype, you can only use this type when calling update 
/// you need to call change_property_type to change this type
#[unsafe(no_mangle)]
pub extern "C" fn create_property(handle: *mut PluginHandle, name: *mut c_char, prop_handle: PropertyHandle, value: Property) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);
    let msg = get_string!(name, DataStoreReturnCode::ParameterCorrupted);

    if let Some(prop_hash) = utils::generate_property_name_hash(msg.as_str()) {
        if prop_handle.property != prop_hash {
            return DataStoreReturnCode::ParameterCorrupted;
        } else if prop_handle.plugin != han.id {
            return DataStoreReturnCode::NotAuthenticated;
        }
    } else {
        return DataStoreReturnCode::ParameterCorrupted;
    }

    if han.properties.contains_key(&prop_handle.property) {
        // Id is already registered
        return DataStoreReturnCode::AlreadyExists;
    }

    let prop_container = utils::PropertyContainer::new(msg, value, han);
    han.sender.send(LoaderMessage::PropertyCreate(prop_handle.property, prop_container)).into()
}

/// Updates the value for the Property behind a given handle
/// 
/// You can only use values of the same type as the inital value (except for arrays).
/// This method can NOT change the type, call change_property_type for this.
///
/// Arrays can NOT be updated by passing in a new array, you can get the handle via get_property
/// and update the individual values.
/// If you can want to change the size or datatype you have to use change_property_type too.
/// Passing in an Array will not deallocate that pointer.
#[unsafe(no_mangle)]
pub extern  "C" fn update_property(handle: *mut PluginHandle, prop_handle: PropertyHandle, value: Property) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    if let Some(entry) = han.properties.get(&prop_handle.property) {
        if entry.update(value, han) {
            return DataStoreReturnCode::Ok;
        } else {
            return DataStoreReturnCode::TypeMissmatch;
        }
    }

    DataStoreReturnCode::DoesNotExist
}

/// Returns the value for a given property handle that you previously subscribed to (or that you
/// created)
#[unsafe(no_mangle)]
pub extern "C" fn get_property_value(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> ReturnValue<Property> {
    let han = get_handle_val!(handle);

    ReturnValue::from(if prop_handle.plugin == han.id {
        // Values we created are also accessible
        if let Some(cont) = han.properties.get(&prop_handle.property) {
            Ok(cont.read())
        } else {
            Err(DataStoreReturnCode::DoesNotExist)
        }
    } else if let Some(store) = han.subscriptions.get(&prop_handle) {
        // As we first checked for those we own, we can garantee we are not allowed to edit these
        // This makes subscribing to you own properties pointless
        Ok(store.read(false))
    } else {
        Err(DataStoreReturnCode::DoesNotExist)
    })
}

/// Generates the PropertyHandle for a certain name
/// 
/// It is advisable to generate these PropertyHandles at Compile time (macro etc) where possible to avoid
/// having to allocate and deallocate a string.
///
/// Name convention is:
/// - At least one dot
/// - Anything ahead of the first dot is the plugin name
/// - Plugin name can not be empty
/// - Case insensitive
/// - More dots can be used
///
/// Similar to create_property, it is your job to deallocate the nullterminating string
#[unsafe(no_mangle)]
pub extern "C" fn generate_property_handle(name: *mut c_char) -> ReturnValue<PropertyHandle> {
    let msg = get_string!(name);
    
    ReturnValue::from(
        PropertyHandle::new(msg.as_str())
        .ok_or(DataStoreReturnCode::ParameterCorrupted)
    )
}

/// Deletes a certain property based on the Handle (or at least queues it)
///
/// Same as create, this (after checking that the property exists) will send a Message to the loader
/// which locks the plugin to perform the delete. The queue length is unknown, so it can take
/// multiple locks and unlocks till this action is performed
#[unsafe(no_mangle)]
pub extern "C" fn delete_property(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    if prop_handle.plugin == han.id && han.properties.contains_key(&prop_handle.property) {
        han.sender.send(LoaderMessage::PropertyDelete(prop_handle.property)).into()
    } else {
        DataStoreReturnCode::DoesNotExist
    }
}

/// This changes the type of a property (more like queues the action)
///
/// Same as create and delete, this (after checking that the property exists) will send a Message to the loader
/// which locks the plugin to perform the change over. The queue length is unknown, so it can take
/// multiple locks and unlocks till this action is performed
#[unsafe(no_mangle)]
pub extern "C" fn change_property_type(handle: *mut PluginHandle, prop_handle: PropertyHandle, value: Property) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    if prop_handle.plugin == han.id && han.properties.contains_key(&prop_handle.property) {
        let cont = utils::ValueContainer::new(value, han);

        han.sender.send(LoaderMessage::PropertyTypeChange(prop_handle.property, cont, true)).into()
    } else {
        DataStoreReturnCode::DoesNotExist
    }
}

/// Subscribes you to a property (or more like queues the action)
/// After this finishes you can access this property through get_property_value
///
/// Similar to create/delete/change_type, this queues the subscribe action.
/// However, in this case do not know if the property we are trying to add exists, as we send a
/// message to our pluginloader, which will then look up and send a message to loader of the plugin
/// for this property, then this respondes back to our loader, which will then add it to the
/// subscriptions (for which it will lock)
#[unsafe(no_mangle)]
pub extern "C" fn subscribe_property(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    // TODO: Remove ability to subscribe to your own properties, as it is pointless
    
    han.sender.send(LoaderMessage::Subscribe(prop_handle)).into()
}

/// Removes subscription for a certain property (it will queue it)
///
/// Same as create/change_property/delete, this (after checking that the property was subscribed to) will send a Message to the loader
/// which locks the plugin to perform the removal. The queue length is unknown, so it can take
/// multiple locks and unlocks till this action is performed
#[unsafe(no_mangle)]
pub extern "C" fn unsubscribe_property(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    if !han.subscriptions.contains_key(&prop_handle) {
        return DataStoreReturnCode::DoesNotExist;
    }
    
    han.sender.send(LoaderMessage::Unsubscribe(prop_handle)).into()
}

/// Generates the EventHandle for a certain name
/// 
/// It is advisable to generate these EventHandles at Compile time (macro etc) where possible to avoid
/// having to allocate and deallocate a string.
///
/// Name convention is:
/// - At least one dot
/// - Anything ahead of the first dot is the plugin name
/// - Plugin name can not be empty
/// - Case insensitive
/// - More dots can be used
///
/// Similar to create_property, it is your job to deallocate the nullterminating string
#[unsafe(no_mangle)]
pub extern "C" fn generate_event_handle(name: *mut c_char) -> ReturnValue<EventHandle> {
    let msg = get_string!(name);
    
    ReturnValue::from(
        EventHandle::new(msg.as_str())
        .ok_or(DataStoreReturnCode::ParameterCorrupted)
    )
}


/// Creates a new Event (if it doesn't exists already).
///
/// This is done by sending a message to the event loop, so we don't know if the event already
/// exists, and it may take time to be created.
/// Also you can only create events from your plugin.
///
/// But as all Event related calls go through the event loop it is guaranteed that the event
/// exists for any trigger calls following this function
#[unsafe(no_mangle)]
pub extern "C" fn create_event(handle: *mut PluginHandle, event: EventHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    if han.id != event.plugin {
        return DataStoreReturnCode::NotAuthenticated;
    }

    han.event_channel.send(EventMessage::Create(event)).into()
}

/// Deletes a Event.
///
/// This is done by sending a message to the event loop, so we don't know if the event even
/// existed, and it may take time to execute.
/// Also you can only delete events from your plugin.
///
/// But as all Event related calls go through the event loop it is guaranteed that the event
/// will not exist for any event related calls after this function
#[unsafe(no_mangle)]
pub extern "C" fn delete_event(handle: *mut PluginHandle, event: EventHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    if han.id != event.plugin {
        return DataStoreReturnCode::NotAuthenticated;
    }

    han.event_channel.send(EventMessage::Remove(event)).into()
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
#[unsafe(no_mangle)]
pub extern "C" fn subscribe_event(handle: *mut PluginHandle, event: EventHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    han.event_channel.send(EventMessage::Subscribe(event, han.id, han.sender.clone().to_async())).into()
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
#[unsafe(no_mangle)]
pub extern "C" fn unsubscribe_event(handle: *mut PluginHandle, event: EventHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    han.event_channel.send(EventMessage::Unsubscribe(event, han.id)).into()
}

/// Triggers an event
///
/// It sends a message to the event loop, so there is no confirmation that your event exists.
///
/// While there can be delays befor execution, but creation/deletion/other trigger calls are
/// guaranteed to not be reordered
#[unsafe(no_mangle)]
pub extern "C" fn trigger_event(handle: *mut PluginHandle, event: EventHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    if han.id != event.plugin {
        return DataStoreReturnCode::NotAuthenticated;
    }

    han.event_channel.send(EventMessage::Trigger(event)).into()
}

/// Creates a new action handle
///
/// Same as EventHandle and PropertyHandle these are stable hashes, so are best generated during
/// compiletime, but you don't need to register/create actions, but have the hash for pattern
/// matching is pretty neat.
///
/// Name convention is:
/// - At least one dot
/// - Anything ahead of the first dot is the plugin name
/// - Plugin name can not be empty
/// - Case insensitive
/// - More dots can be used
///
/// Similar to other functions, it is your job to deallocate the nullterminating string
#[unsafe(no_mangle)]
pub extern "C" fn generate_action_handle(name: *mut c_char) -> ReturnValue<ActionHandle> {
    let msg = get_string!(name);

    ReturnValue::from(
        ActionHandle::new(msg.as_str())
            .ok_or(DataStoreReturnCode::ParameterCorrupted)
    )
}

/// This triggers the given Action (defined by the ActionHandle) with the parameters, based on
/// params being a standard C array with length param_count.
///
/// If you don't want to pass any parameters then you can simply set params to null and param_count
/// to 0. It is otherwise trusted that you give a valid pointer to an array of this length, any
/// deviations will result in at best memory leaks at worst SegFaults.  
/// Deallocating the Array will be the job of the receiver/action runner, so do NOT deallocate the
/// array, or you will create a use after free or double free.
///
/// You will get the action id returned, this is a unique id (across all plugins, only restart when
/// Datarace restarts) that uniformly climbs, so newer ids are larger (it could overflow, but even
/// at 1,000,000 actions per second it would take 580k years).  
/// The id is used to identify the ActionCallback.
#[unsafe(no_mangle)]
pub extern "C" fn trigger_action(handle: *mut PluginHandle, action_handle: ActionHandle, params: *mut Property, param_count: usize) -> ReturnValue<u64> {
    let han = get_handle_val!(handle);

    let action = Action::new(han.id, action_handle.action, params, param_count);

    let res = futures_lite::future::block_on(async {
        let ds_r = han.datastore.read().await;
        
        let res = ds_r.trigger_action(action_handle.plugin, action).await;

        drop(ds_r);
        res
    });

    ReturnValue::from(res.ok_or(DataStoreReturnCode::ParameterCorrupted))
}

/// This triggers the callback for a given Action, with the given return_code and parameters.
///
/// Like trigger_action, params is a C Array that must be the length of param_count, and if you
/// don't want to pass anything pass null/0 for params/param_count.
///
/// The return_code should be 0 for success, however you may use others at your own discression,
/// just make sure you document them.
///
/// The previous_action is consumed by this call, and it's params deallocated, so do not deallocate
/// manually (or, replace the params in the action with null prior to calling this function).
#[unsafe(no_mangle)]
pub extern "C" fn action_callback(handle: *mut PluginHandle, previous_action: Action, return_code: u64, params: *mut Property, param_count: usize) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    let mut action = Action::new(han.id, return_code, params, param_count);
    action.id = previous_action.id;


    let res = futures_lite::future::block_on(async {
        let ds_r = han.datastore.read().await;
        
        let res = ds_r.callback_action(previous_action.origin, action).await;

        drop(ds_r);
        res
    });

    unsafe {
        previous_action.dealloc();
    }


    if res {
        DataStoreReturnCode::Ok
    } else {
        DataStoreReturnCode::ParameterCorrupted
    }
}

/// Logs a null terminated String as a Info
/// String is not deallocated, that is your job
#[unsafe(no_mangle)]
pub extern "C" fn log_info(handle: *mut PluginHandle, message: *mut c_char) {
    log_plugin_msg(handle, message, log::Level::Info);
}

/// Logs a null terminated String as a Error
/// String is not deallocated, that is your job
#[unsafe(no_mangle)]
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

/// This returns the ptr to a state you stored earlier,
/// allowing you to have shared state in your plugin
#[unsafe(no_mangle)]
pub extern "C" fn get_state(handle: *mut PluginHandle) -> *mut c_void {
    let han = get_handle!(handle, std::ptr::null_mut());

    han.state_ptr
}

/// This writes the state ptr immediatly
///
/// will aquire a lock while writing in the ptr, but reads will not be blocked and will cause
/// undefined behavior. In general, you should probably write this only once during init, after
/// that just read the value and rely on intirior mutability.
///
/// It is also your responsibility to deallocate the memory.
/// Currently this is difficult, while Shutdown is signaled, and you could deallocate it then
/// (but also, as the programm is shutting down, we could leak it briefly before the os cleans up,
/// but this behavior may change in future releases to allow partial shutdown/restarts),
/// if your plugin suffered an error (especially one that crashed the loader task too)
/// we have no way to dispose it
#[unsafe(no_mangle)]
pub extern "C" fn save_state_now(handle: *mut PluginHandle, state: *mut c_void) {
    let han = get_handle!(handle);

    han.lock();
    {
        let han = if let Some(han) = unsafe {
            handle.as_mut()
        } {
            han
        } else {
            han.unlock();
            return;
        };

        han.state_ptr = state;
    }
    han.unlock();
}

/// Gets a Value at a certain index in this array.
///
/// If the index is out of bounds returns a Property with Type None
#[unsafe(no_mangle)]
pub extern "C" fn get_array_value(array_handle: *mut ArrayValueHandle, index: usize) -> Property {
    let arr = if let Some(arr) = unsafe {
        array_handle.as_ref()  
    } {
        arr
    } else {
        return Property::default();
    };

    arr.arr.read(index)
}

/// Sets the Value at a certain index of an array.
///
/// This value must be the same type as all other values in the array.
/// If you intend to change this (or resize the array) you need to replace the array.
///
/// You can only edit arrays you created.
/// Trying to change value in Arrayhandles from properties of other plugin will return NotAuthenticated
#[unsafe(no_mangle)]
pub extern "C" fn set_array_value(handle: *mut PluginHandle, array_handle: *mut ArrayValueHandle, index: usize, value: Property) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);
    let arr = if let Some(arr) = unsafe {
        array_handle.as_ref()  
    } {
        arr
    } else {
        return DataStoreReturnCode::ParameterCorrupted;
    };

    if arr.allow_modify {
        arr.arr.write(index, value, han)
    } else {
        DataStoreReturnCode::NotAuthenticated
    }
}

/// Returns the length of the array
#[unsafe(no_mangle)]
pub extern "C" fn get_array_length(array_handle: *mut ArrayValueHandle) -> usize {
    let arr = if let Some(arr) = unsafe {
        array_handle.as_ref()  
    } {
        arr
    } else {
        return 0;
    };

    arr.arr.length()
}

/// Returns the type for the data stored in the array
#[unsafe(no_mangle)]
pub extern "C" fn get_array_type(array_handle: *mut ArrayValueHandle) -> PropertyType {
    let arr = if let Some(arr) = unsafe {
        array_handle.as_ref()  
    } {
        arr
    } else {
        return PropertyType::None;
    };

    arr.arr.get_type()
}

/// Creates a new Array and returns it's handle.
///
/// Only Int, Float, Bool, String, Duration are accepted as types, others will fail.
/// This function will return null on fail.
///
/// Size and type can not be changed later.
/// Additionally you can not index into this array like a regular C array(as it is a wrapper around
/// a reference counted object), use `set_array_value` and `get_array_value` respectivly.
///
/// When putting this ArrayHandle into a Property and sending it off to `create_property` or `change_property_type`
/// then this pointer is consumed, you should call `clone_array_handle` first (or get the new
/// handle from the property).
///
/// When the handle goes out of scope make sure to call `drop_array_handle`, this will only
/// deallocate if you were the last holding it.
#[unsafe(no_mangle)]
pub extern "C" fn create_array(handle: *mut PluginHandle, size: usize, init_value: Property) -> *mut ArrayValueHandle {
    let han = get_handle!(handle, std::ptr::null_mut());

    if let Some(arr) = utils::ArrayValueContainer::new(size, init_value, han) {
        let arr_handle = ArrayValueHandle { arr: Arc::new(arr), allow_modify: true };

        Box::into_raw(Box::new(arr_handle))
    } else {
        std::ptr::null_mut()
    }
}

/// Dublicates the array handle (without deallocating the passed in handle).
///
/// These two handles access the same array.
/// Useful for parallel execution.
///
/// Be aware to call `drop_array_handle` precisely once on each handle
#[unsafe(no_mangle)]
pub extern "C" fn clone_array_handle(array_handle: *mut ArrayValueHandle) -> *mut ArrayValueHandle {
    let arr = if let Some(arr) = unsafe {
        array_handle.as_ref()  
    } {
        arr
    } else {
        return std::ptr::null_mut()
    };

    let dub = ArrayValueHandle { arr: arr.arr.clone(), allow_modify: arr.allow_modify.clone() };
    Box::into_raw(Box::new(dub))
}

/// Drops the passed in ArrayHandle.
///
/// This does not necessarily drop the array, only if this was the last handle holding it (and no property is holding it)
#[unsafe(no_mangle)]
pub extern "C" fn drop_array_handle(array_handle: *mut ArrayValueHandle) {
    if !array_handle.is_null() {
        unsafe {
            array_handle.drop_in_place()
        }
    }
}



/// Sends a message to the update function of your plugin.  
/// This type of internal message is useful for sending messages from worker threads, for example
/// that they failed, so you could restart them or shut the plugin down
#[unsafe(no_mangle)]
pub extern "C" fn send_internal_msg(handle: *mut PluginHandle, msg_code: i64) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    han.sender.send(LoaderMessage::InternalMessage(msg_code)).into()
}

/// Allows you to send a raw memory pointer to another plugin.  
/// The target is plugin id of the target plugin.  
/// reason serves as a way to communicate what this pointer is for, although the recipient is also
/// told your plugin id.  
/// Obviously managing void pointers is risky business, both recipients have to be on the same
/// package and understand what it stands for.
#[unsafe(no_mangle)]
pub extern "C" fn send_ptr_msg_to_plugin(handle: *mut PluginHandle, target: u64, ptr: *mut c_void, reason: i64) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    han.sender.send(LoaderMessage::SendPluginMessagePtr((target, VoidPtrWrapper { ptr }, reason))).into()
}

/// Allows you to optain the id of another plugin based on it's name. 
/// This function is intended for runtime use, for compiletime macros use `compiletime_get_plugin_name_hash()`.
///
/// The name is a nullterminated string that you need to deallocate after.  
///
/// This function also checks if the name does not contain any invalid characters (currently only .),
/// but does not check if the plugin is loaded.
#[unsafe(no_mangle)]
pub extern "C" fn get_foreign_plugin_id(handle: *mut PluginHandle, name: *mut c_char) -> PluginNameHash {
    let _han = get_handle!(handle, PluginNameHash { valid: false, id: 0 });
    // We only aquire a reference to stop people from passing in null
    
    if let Some(str) = utils::get_string(name) {
        let str = str.to_lowercase();
        if let Some(val) = utils::generate_plugin_name_hash(str.as_str()) {
            PluginNameHash { id: val, valid: true }    
        } else {
            PluginNameHash { id: 0, valid: false }
        }
    } else {
        PluginNameHash { id: 0, valid: false }
    }
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
/// Further, you need to make sure to always unlock, this won't unlock automatically when going out
/// of scope/crashing.
/// DO NOT call this function a second time before calling `unlock_plugin()`, this will deadlock,
/// as this function will forever wait for the unlock.
///
/// Do not call this in the update or init function, as this can deadlock the plugin.
#[unsafe(no_mangle)]
pub extern "C" fn lock_plugin(handle: *mut PluginHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    han.lock();
    DataStoreReturnCode::Ok
}

/// This is the other half of to `lock_plugin()`.
/// It is likely a good idea to build a custom data type that calls `lock_plugin()` as a
/// constructor, and this function during it's destructor, to avoid not unlocking the plugin
/// when going out of scope/crashing.  
///
/// It is important to not call this function without previously locking the plugin, as
/// this can cause undefine behavior.
///
/// Also a good idea not to use in the init and update functions.
#[unsafe(no_mangle)]
pub extern "C" fn unlock_plugin(handle: *mut PluginHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    han.unlock();
    DataStoreReturnCode::Ok
}

/// This reloads the Plugin Settings from it's file.
/// 
/// Be aware that any unsaved changes will be lost, including transient Settings Properties.
#[unsafe(no_mangle)]
pub extern "C" fn reload_plugin_settings(handle: *mut PluginHandle) -> PluginSettingsLoadReturn {
    let handle = get_handle!(handle,
        PluginSettingsLoadReturn {
            code: PluginSettingsLoadState::PslsHandleNullPtr,
            fail: PluginSettingsLoadFail { filler: 0 }
        });

    futures_lite::future::block_on(async {
        let mut ds_w = handle.datastore.write().await;
        ds_w.reload_plugin_settings(handle.id).await
    })
}

/// This saves the Plugin Settings from it's file.
/// 
/// Only none transiant properties are saved.
/// Once Saved (and successfull), reload will revert to this state.
///
/// In case of a FileSystem Error it is unclear if a partial write occured, all other errors leave
/// the previous saved file always intact.
#[unsafe(no_mangle)]
pub extern "C" fn save_plugin_settings(handle: *mut PluginHandle) -> PluginSettingsLoadState {
    let handle = get_handle!(handle, PluginSettingsLoadState::PslsHandleNullPtr);

    
    futures_lite::future::block_on(async {
        let ds_r = handle.datastore.read().await;
        ds_r.save_plugin_settings(handle.id).await
    })
}

/// This retrieves a settings property for this plugin.
/// Settings are distinct from standard properties, they are used for the Settings Dashboard for
/// your plugin, and to persist these between restarts. But their access is slow (Requiring the central DataStore to lock), 
/// so you are best off reading these once and caching them.
/// But they do use the same PropertyHandle as regular properties (without name collisions between
/// the two types, so you can have a property and plugin setting property named the same).
///
/// Attempting a access a setting of another plugin will result in NotAuthenticated error.
/// A setting needs to be created first (through create_plugin_settings_proptery, or it was in the
/// settings file loaded on startup/through reload_plugin_settings).
#[unsafe(no_mangle)]
pub extern "C" fn get_plugin_settings_property(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> ReturnValue<Property> {
    let handle = get_handle_val!(handle);

    if prop_handle.plugin != handle.id {
        return ReturnValue::new_from_error(DataStoreReturnCode::NotAuthenticated);
    }

    futures_lite::future::block_on(async {
        let ds_r = handle.datastore.read().await;
        match ds_r.get_plugin_settings_property(handle.id, prop_handle.property) {
            Some(value) => {
                let res = value.value.read(true);
                ReturnValue { code: DataStoreReturnCode::Ok, value: res }
            },
            None => ReturnValue::new_from_error(DataStoreReturnCode::DoesNotExist)
        }
    })
}

/// Creates a plugin settings property.
///
/// Settings are distinct from standard properties, they are used for the Settings Dashboard for
/// your plugin, and to persist these between restarts. But their access is slow (Requiring the central DataStore to lock), 
/// so you are best off reading these once and caching them.
/// But they do use the same PropertyHandle as regular properties.
///
/// Transient Settings are not persevered when the settings are saved.
///
/// Same as create_property, the name of your plugin will be prepended to the final name: plugin_name.name
/// It is also your job to deallocate this name string.
#[unsafe(no_mangle)]
pub extern "C" fn create_plugin_settings_property(
    handle: *mut PluginHandle, 
    prop_handle: PropertyHandle, 
    name: *mut c_char, 
    value: Property,
    transient: bool
) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);
    let msg = get_string!(name, DataStoreReturnCode::ParameterCorrupted);

    if let Some(prop_hash) = utils::generate_property_name_hash(msg.as_str()) {
        if prop_handle.property != prop_hash {
            return DataStoreReturnCode::ParameterCorrupted;
        } else if prop_handle.plugin != han.id {
            return DataStoreReturnCode::NotAuthenticated;
        }
    } else {
        return DataStoreReturnCode::ParameterCorrupted;
    }

    futures_lite::future::block_on(async {
        let mut ds_w = han.datastore.write().await;

        if ds_w.get_plugin_settings_property(han.id, prop_handle.property).is_some() {
            return DataStoreReturnCode::AlreadyExists;
        }

        let prop_container = utils::ValueContainer::new(value, &han);

        ds_w.insert_plugin_settings_property(han.id, prop_handle.property, 
            crate::datastore::PluginSettingProperty { name: msg, value: prop_container, transient }).await
    })
}

/// This changes the value (and possibly the type) of a plugin settings property.
///
/// Compared to the update function for normal properties you can use this function to change type
/// too, the only requirement is that the settings property has to exist first.
#[unsafe(no_mangle)]
pub extern "C" fn change_plugin_settings_property(handle: *mut PluginHandle, prop_handle: PropertyHandle, value: Property) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    if prop_handle.plugin != han.id {
        return DataStoreReturnCode::NotAuthenticated;
    }

    futures_lite::future::block_on(async {
        let mut ds_w = han.datastore.write().await;

        match ds_w.get_plugin_settings_property(han.id, prop_handle.property) {
            Some(setting) => {
                // if setting.value.matching_type(&value) {
                //     setting.value.update(value, han);
                // } else {

                // We recreate the value container
                let transient = setting.transient;
                let name = setting.name.clone();
                
                let prop_container = utils::ValueContainer::new(value, han);

                ds_w.insert_plugin_settings_property(han.id, prop_handle.property, 
                    crate::datastore::PluginSettingProperty { name, value:  prop_container, transient }).await


                // }
            },
            None => DataStoreReturnCode::DoesNotExist
        }
    })
}

/// This deletes a plugin settings property.
///
/// Compared to the delete_property function for normal properties, this delete is finished upon
/// the function returning
#[unsafe(no_mangle)]
pub extern "C" fn delete_plugin_settings_property(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);

    if prop_handle.plugin != han.id {
        return DataStoreReturnCode::NotAuthenticated;
    }

    futures_lite::future::block_on(async {
        let mut ds_w = han.datastore.write().await;

        ds_w.remove_plugin_settings_property(han.id, prop_handle.property).await
    })
}

/// Checks if a Plugin Settings Property is transient (aka will not be saved in the settings file).
///
/// This will return true also on any settings properties that do not exist, or prop handles you
/// have no access to, so you should check if the settings property exists first via
/// get_plugin_settings_property
#[unsafe(no_mangle)]
pub extern "C" fn is_plugin_settings_property_transient(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> bool {
    let han = get_handle!(handle, true);

    if prop_handle.plugin != han.id {
        return true;
    }

    futures_lite::future::block_on(async {
        let ds_r = han.datastore.read().await;

        match ds_r.get_plugin_settings_property(han.id, prop_handle.property) {
            Some(value) => value.transient,
            None => true
        }
    })
}

/// Make a plugin setting property transient (or no longer transient).
///
/// Transient Properties will not be saved in the Settings file, so this enables you to
/// retroactively set a property as transient or vise versa.
#[unsafe(no_mangle)]
pub extern "C" fn set_plugin_settings_property_transient(handle: *mut PluginHandle, prop_handle: PropertyHandle, transient: bool) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);


    if prop_handle.plugin != han.id {
        return DataStoreReturnCode::NotAuthenticated;
    }

    futures_lite::future::block_on(async {
        let mut ds_w = han.datastore.write().await;

        ds_w.set_plugin_settings_property_transient(han.id, prop_handle.property, transient)
    })
}



/// Puts a message back into the Queue (currently not implemented)
///
/// Keep in mind, if you reenque any Message this will alter the order, and may result in, for
/// example, actions being performed in a different order then triggered. 
/// Also DataStoreLock messages can not be enqueed.
///
/// Part of the point of this function is so the Message type is included in the generated header
#[unsafe(no_mangle)]
pub extern "C" fn reenqueue_message(handle: *mut PluginHandle, msg: Message) -> DataStoreReturnCode {
    let _han = get_handle!(handle, DataStoreReturnCode::HandleNullPtr);
    let _msg = msg;
    
    // // need to reencode Message
    // let re_coded = match msg.sort {
    //     MessageType::Update => {
    //         unsafe {
    //             let mut msg = msg;
    //             let up = ManuallyDrop::into_inner(std::mem::take(&mut msg.value.update));
    //
    //             pluginloader::Message::Update(up.handle, utils::Value::new(up.value))
    //         }
    //     },
    //     MessageType::Removed => {
    //         unsafe {
    //             let handle = msg.value.removed_property;
    //             pluginloader::Message::Removed(handle)
    //         }
    //     }
    // };
    //
    // // we have to retrieve this plugins channel
    // let res = futures::executor::block_on(async {
    //     let ds = han.datastore.read().await;
    //     if let Some(chan) = ds.get_plugin_channel(&han.name).await {
    //         if chan.send(re_coded).await.is_ok() {
    //             DataStoreReturnCode::Ok
    //         } else {
    //             DataStoreReturnCode::DoesNotExist
    //         }
    //     } else {
    //         DataStoreReturnCode::NotAuthenticated
    //     }
    // });
    // res
    DataStoreReturnCode::NotImplemented
}

/// This returns the descriptor of our plugin <br>
/// There is a string contained, requiring deallocation
/// 
/// Part of the point of this function is so the PluginDescription type is included in the generated header
#[unsafe(no_mangle)]
pub extern "C" fn get_description(handle: *mut PluginHandle) -> PluginDescription {
    let han = get_handle!(handle, PluginDescription {
        id: 0,
        name: std::ptr::null_mut(),
        version: [0,0,0],
        api_version: API_VERSION,
    });

    
    PluginDescription {
        id: han.id,
        name: std::ffi::CString::new(han.name.clone()).expect("string is string").into_raw(),
        version: han.version.clone(),
        api_version: API_VERSION,
    }
}

/// It is the proper way to let every library deallocate memory it allocated.
/// So this function is provided to allow you to deallocate strings the API passed to you
#[unsafe(no_mangle)]
pub extern "C" fn deallocate_string(ptr: *mut libc::c_char) {
    unsafe {
        drop(std::ffi::CString::from_raw(ptr))
    }
}
