use libc::c_char;
use log::error;

use crate::{pluginloader::LoaderMessage, utils, DataStoreReturnCode, Message, PluginDescription, PluginHandle, Property, PropertyHandle, ReturnValue, API_VERSION};


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
#[no_mangle]
pub extern "C" fn create_property(handle: *mut PluginHandle, name: *mut c_char, prop_handle: PropertyHandle, value: Property) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);
    let msg = get_string!(name, DataStoreReturnCode::ParameterCorrupted);

    if let Some(prop_hash) = utils::generate_property_name_hash(msg.as_str()) {
        if prop_handle.property != prop_hash || prop_handle.plugin != han.id {
            return DataStoreReturnCode::ParameterCorrupted;
        }
    } else {
        return DataStoreReturnCode::ParameterCorrupted;
    }

    if han.properties.contains_key(&prop_handle.property) {
        // Id is already registered
        return DataStoreReturnCode::AlreadyExists;
    }

    let prop_container = utils::PropertyContainer::new(msg, value, han);
    if let Err(e) = han.sender.send(LoaderMessage::PropertyCreate(prop_handle.property, prop_container)) {
        error!("Failed to send message in channel for Plugin {}: {}", han.name, e);
        return DataStoreReturnCode::DataCorrupted; // TODO new type for a not total fail error
    }
    

    DataStoreReturnCode::Ok
}

/// Updates the value for the Property behind a given handle
/// 
/// You can only use values of the same type as the inital value
/// This method can NOT change the type, call change_property_type for this
#[no_mangle]
pub extern  "C" fn update_property(handle: *mut PluginHandle, prop_handle: PropertyHandle, value: Property) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);

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
#[no_mangle]
pub extern "C" fn get_property_value(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> ReturnValue<Property> {
    let han = get_handle_val!(handle);

    ReturnValue::from(if let Some(store) = han.subscriptions.get(&prop_handle) {
        Ok(store.read())
    } else if prop_handle.plugin == han.id {
        // Values we created are also accessible
        if let Some(cont) = han.properties.get(&prop_handle.property) {
            Ok(cont.read())
        } else {
            Err(DataStoreReturnCode::DoesNotExist)
        }
    } else {
        Err(DataStoreReturnCode::DoesNotExist)
    })
}

/// Generates the PropertyHandle for a certain name
/// 
/// Similar to create_property, it is your job to deallocate the nullterminating string
/// It is advisable to generate these PropertyHandles at Compile time where possible to avoid
/// having to allocate and deallocate a string.
///
/// It is a good idea to use compile time macros (if your language supports them) to generate the
/// handles during compiletime. This allows to cut down on runtime overhead from calling this
/// function (and other overhead from having to allocate memory to do so too)
#[no_mangle]
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
#[no_mangle]
pub extern "C" fn delete_property(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);

    if prop_handle.plugin == han.id && han.properties.contains_key(&prop_handle.property) {
        if let Err(e) = han.sender.send(LoaderMessage::PropertyDelete(prop_handle.property)) {
            error!("Failed to send message in channel for Plugin {}: {}", han.name, e);
            DataStoreReturnCode::DataCorrupted
        } else {
            DataStoreReturnCode::Ok
        }
    } else {
        DataStoreReturnCode::DoesNotExist
    }
}

/// This changes the type of a property (more like queues the action)
///
/// Same as create and delete, this (after checking that the property exists) will send a Message to the loader
/// which locks the plugin to perform the change over. The queue length is unknown, so it can take
/// multiple locks and unlocks till this action is performed
#[no_mangle]
pub extern "C" fn change_property_type(handle: *mut PluginHandle, prop_handle: PropertyHandle, value: Property) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);

    if prop_handle.plugin == han.id && han.properties.contains_key(&prop_handle.property) {
        let cont = utils::ValueContainer::new(value, han);

        if let Err(e) = han.sender.send(LoaderMessage::PropertyTypeChange(prop_handle.property, cont, true)) {
            error!("Failed to send message in channel for Plugin {}: {}", han.name, e);
            DataStoreReturnCode::DataCorrupted
        } else {
            DataStoreReturnCode::Ok
        }
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
#[no_mangle]
pub extern "C" fn subscribe_property(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);

    log::debug!("Hit subscribe");
    
    if let Err(e) = han.sender.send(LoaderMessage::Subscribe(prop_handle)) {
        error!("Failed to send message in channel for Plugin {}: {}", han.name, e);
        DataStoreReturnCode::DataCorrupted
    } else {
        DataStoreReturnCode::Ok
    }
}

/// Removes subscription for a certain property (it will queue it)
///
/// Same as create/change_property/delete, this (after checking that the property was subscribed to) will send a Message to the loader
/// which locks the plugin to perform the removal. The queue length is unknown, so it can take
/// multiple locks and unlocks till this action is performed
#[no_mangle]
pub extern "C" fn unsubscribe_property(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);

    if !han.subscriptions.contains_key(&prop_handle) {
        return DataStoreReturnCode::DoesNotExist;
    }
    
    if let Err(e) = han.sender.send(LoaderMessage::Unsubscribe(prop_handle)) {
        error!("Failed to send message in channel for Plugin {}: {}", han.name, e);
        DataStoreReturnCode::DataCorrupted
    } else {
        DataStoreReturnCode::Ok
    }
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

/// Puts a message back into the Queue
///
/// Keep in mind, if you reenque an Update message, this may result in another value update for
/// this property coming inbetween, resulting in you progressing next the newer value before the
/// reenqueued value
///
/// Part of the point of this function is so the Message type is included in the generated header
#[no_mangle]
pub extern "C" fn reenqueue_message(handle: *mut PluginHandle, msg: Message) -> DataStoreReturnCode {
    let _han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);
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
#[no_mangle]
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
#[no_mangle]
pub extern "C" fn deallocate_string(ptr: *mut libc::c_char) {
    unsafe {
        drop(std::ffi::CString::from_raw(ptr))
    }
}
