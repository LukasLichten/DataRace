// A lot of this is due to importing large sets of C standard lib
#[allow(dead_code,non_upper_case_globals,non_camel_case_types,improper_ctypes,non_snake_case)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

#[cfg(feature = "main-entry")]
pub use bindings::run;

//Functions
// Log Functions
pub use bindings::{log_info, log_error};

// Property Functions
pub use bindings::{create_property, update_property, get_property_value, generate_property_handle, delete_property, change_property_type, subscribe_property, unsubscribe_property};

//Additional functions
pub use bindings::deallocate_string;
pub use bindings::{get_foreign_plugin_id, send_ptr_msg_to_plugin, send_internal_msg};

//State functions
pub use bindings::{save_state_now, get_state};

//Lock functions
pub use bindings::{lock_plugin, unlock_plugin};

//Data
// Enums
pub use bindings::{DataStoreReturnCode, DataStoreReturnCode_Ok, DataStoreReturnCode_NotAuthenticated, DataStoreReturnCode_AlreadyExists, DataStoreReturnCode_DoesNotExist, DataStoreReturnCode_TypeMissmatch, DataStoreReturnCode_NotImplemented, DataStoreReturnCode_ParameterCorrupted, DataStoreReturnCode_DataCorrupted};
pub use bindings::{PropertyType, PropertyType_None, PropertyType_Int, PropertyType_Float, PropertyType_Boolean, PropertyType_Str, PropertyType_Duration};
pub use bindings::{MessageType, MessageType_InternalMessage, MessageType_StartupFinished, MessageType_OtherPluginStarted, MessageType_PluginMessagePtr, MessageType_Lock, MessageType_Unlock, MessageType_Shutdown}; 

// Message
pub use bindings::{Message, MessageValue};
pub use bindings::{UpdateValue, MessagePtr};
pub use bindings::reenqueue_message;

// Property
pub use bindings::{Property, PropertyValue, PropertyHandle};

// Plugins
pub use bindings::{PluginHandle,PluginDescription};

// ReturnValues
pub use bindings::{ReturnValue_PropertyHandle, ReturnValue_Property};
pub use bindings::PluginNameHash;

// Compiletime
#[cfg(feature = "compile")]
pub use bindings::{ compiletime_get_api_version, compiletime_get_plugin_name_hash};
