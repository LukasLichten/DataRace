// A lot of this is due to importing large sets of C standard lib
#[allow(dead_code, non_upper_case_globals)]
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
pub use bindings::{create_array, get_array_value, set_array_value, clone_array_handle, drop_array_handle, get_array_length, get_array_type};

// Events
pub use bindings::{generate_event_handle, create_event, delete_event, subscribe_event, unsubscribe_event, trigger_event};

// Actions
pub use bindings::{generate_action_handle, trigger_action, action_callback};

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
pub use bindings::{PropertyType, PropertyType_None, PropertyType_Int, PropertyType_Float, PropertyType_Boolean, PropertyType_Str, PropertyType_Duration, PropertyType_Array};
pub use bindings::{MessageType, MessageType_InternalMessage, MessageType_StartupFinished, MessageType_OtherPluginStarted, MessageType_PluginMessagePtr, MessageType_Lock, MessageType_Unlock, MessageType_Shutdown, MessageType_EventTriggered, MessageType_EventUnsubscribed, MessageType_ActionRecv, MessageType_ActionCallback}; 

// Message
pub use bindings::{Message, MessageValue};
pub use bindings::{MessagePtr, Action};
pub use bindings::reenqueue_message;

// Property
pub use bindings::{Property, PropertyValue, PropertyHandle, ArrayValueHandle, ActionHandle};

// Event
pub use bindings::EventHandle;

// Plugins
pub use bindings::{PluginHandle,PluginDescription};

// ReturnValues
pub use bindings::{ReturnValue_PropertyHandle, ReturnValue_Property, ReturnValue_EventHandle, ReturnValue_u64};
pub use bindings::PluginNameHash;

// Compiletime
#[cfg(feature = "compile")]
pub use bindings::{ compiletime_get_api_version, compiletime_get_plugin_name_hash};
