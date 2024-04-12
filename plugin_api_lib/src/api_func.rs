use libc::c_char;
use log::error;

use crate::{pluginloader, utils, DataStoreReturnCode, Message, MessageType, PluginDescription, PluginHandle, Property, PropertyHandle, ReturnValue, API_VERSION};


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

/// Creates a new property, and returns (if it succeeds) the PropertyHandle of this Property
///
/// Keep in mind, the name of your plugin will be prepended to the final name: plugin_name.name
/// Also the initial value set the datatype, you can only use this type when calling update 
/// you need to call change_property_type to change this type
#[no_mangle]
pub extern "C" fn create_property(handle: *mut PluginHandle, name: *mut c_char, prop_handle: PropertyHandle, value: Property) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);
    let msg = get_string!(name, DataStoreReturnCode::ParameterCorrupted);

    if let Some(prop_hash) = utils::generate_property_name_hash(msg.as_str()) {
        if prop_handle.property != prop_hash || prop_handle.plugin != han.id {
            // TODO perhaps a new error code for
            // invalid parameters, but not corrupted
            return DataStoreReturnCode::ParameterCorrupted;
        }
    } else {
        return DataStoreReturnCode::ParameterCorrupted;
    }

    let _prop_container = utils::PropertyContainer::new(msg, value, han);
    // TODO message the pluginloader to add the property

    DataStoreReturnCode::Ok
}

/// Updates the value for the Property behind a given handle
/// 
/// You can only use values of the same type as the inital value
/// This method can NOT change the type, call change_property_type for this
#[no_mangle]
pub extern  "C" fn update_property(handle: *mut PluginHandle, prop_handle: PropertyHandle, value: Property) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);

    let val = utils::Value::new(value);
    

    DataStoreReturnCode::NotImplemented
}

/// Returns the value for a given property handle that you previously subscribed to
/// 
#[no_mangle]
pub extern "C" fn get_property_value(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> ReturnValue<Property> {
    let han = get_handle_val!(handle);

    // let res = futures::executor::block_on(async {
    //     let ds = han.datastore.read().await;
    //     ds.get_property(&prop_handle).await
    // });
    //
    // ReturnValue::from(match res {
    //     Ok(val) => {
    //         Property::try_from(val)
    //     },
    //     Err(e) => Err(e)
    // })

    ReturnValue::from(Err(DataStoreReturnCode::NotImplemented))
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

/// Deletes a certain property based on the Handle
#[no_mangle]
pub extern "C" fn delete_property(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);

    DataStoreReturnCode::NotImplemented
}

/// Subscribes you to a property, allowing fast access through get_property_value
#[no_mangle]
pub extern "C" fn subscribe_property(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);

    let res = futures::executor::block_on(async {
        let mut ds = han.datastore.read().await;
        DataStoreReturnCode::NotImplemented
    });
    res
}

/// Removes subscription off this plugin from a certain property
///
/// You may after this call still receive some messages from updates of this property for a brief
/// time as the message queue is emptied
#[no_mangle]
pub extern "C" fn unsubscribe_property(handle: *mut PluginHandle, prop_handle: PropertyHandle) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);

    let res = futures::executor::block_on(async {
        let mut ds = han.datastore.read().await;
        DataStoreReturnCode::NotImplemented
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

/// Puts a message back into the Queue
///
/// Keep in mind, if you reenque an Update message, this may result in another value update for
/// this property coming inbetween, resulting in you progressing next the newer value before the
/// reenqueued value
///
/// Part of the point of this function is so the Message type is included in the generated header
#[no_mangle]
pub extern "C" fn reenqueue_message(handle: *mut PluginHandle, msg: Message) -> DataStoreReturnCode {
    let han = get_handle!(handle, DataStoreReturnCode::DataCorrupted);
    
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
        version: [0,0,0],
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
