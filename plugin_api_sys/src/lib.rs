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

// Additional functions
pub use bindings::deallocate_string;
pub use bindings::{get_foreign_plugin_id, send_ptr_msg_to_plugin, send_internal_msg};

// State functions
pub use bindings::{save_state_now, get_state};

// Lock functions
pub use bindings::{lock_plugin, unlock_plugin};

// Settings functions
pub use bindings::{reload_plugin_settings, save_plugin_settings, set_plugin_settings_property_transient, is_plugin_settings_property_transient, change_plugin_settings_property, create_plugin_settings_property, get_plugin_settings_property, delete_plugin_settings_property};

//Data
// Enums
pub use bindings::{DataStoreReturnCode, DataStoreReturnCode_Ok, DataStoreReturnCode_NotAuthenticated, DataStoreReturnCode_AlreadyExists, DataStoreReturnCode_DoesNotExist, DataStoreReturnCode_TypeMissmatch, DataStoreReturnCode_NotImplemented, DataStoreReturnCode_ParameterCorrupted, DataStoreReturnCode_HandleNullPtr, DataStoreReturnCode_InternalError, DataStoreReturnCode_InternalChannelClosed, DataStoreReturnCode_InternalChannelReceiverClosed};
pub use bindings::{PropertyType, PropertyType_None, PropertyType_Int, PropertyType_Float, PropertyType_Boolean, PropertyType_Str, PropertyType_Duration, PropertyType_Array};
pub use bindings::{MessageType, MessageType_InternalMessage, MessageType_StartupFinished, MessageType_OtherPluginStarted, MessageType_PluginMessagePtr, MessageType_Lock, MessageType_Unlock, MessageType_Shutdown, MessageType_EventTriggered, MessageType_EventUnsubscribed, MessageType_ActionRecv, MessageType_ActionCallback}; 
pub use bindings::{PluginSettingsLoadState, PluginSettingsLoadState_Loaded, PluginSettingsLoadState_NoFile, PluginSettingsLoadState_VersionNewerThenCurrent, PluginSettingsLoadState_VersionOlderThenCurrent,
PluginSettingsLoadState_JsonParseError, PluginSettingsLoadState_FileSystemError, PluginSettingsLoadState_PslsHandleNullPtr};

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
pub use bindings::{ReturnValue_PropertyHandle, ReturnValue_Property, ReturnValue_EventHandle, ReturnValue_u64, ReturnValue_ActionHandle};
pub use bindings::PluginNameHash;
pub use bindings::{PluginSettingsLoadReturn, PluginSettingsLoadFail};

// Compiletime
#[cfg(feature = "compile")]
pub use bindings::{ compiletime_get_api_version, compiletime_get_plugin_name_hash};

#[cfg(test)]
mod test {
    use crate::{Action, ActionHandle, DataStoreReturnCode, EventHandle, Message, MessagePtr, MessageType, MessageValue, PluginDescription, PluginSettingsLoadFail, PluginSettingsLoadReturn, Property, PropertyHandle, PropertyType, PropertyValue, ReturnValue_ActionHandle, ReturnValue_EventHandle, ReturnValue_Property, ReturnValue_PropertyHandle, ReturnValue_u64};

    #[test]
    fn abi_size_check() {
        assert_eq!(std::mem::size_of::<ActionHandle>(), 16, "ActionHandle size missmatch");
        assert_eq!(std::mem::size_of::<PropertyHandle>(), 16, "PropertyHandle size missmatch");
        assert_eq!(std::mem::size_of::<EventHandle>(), 16, "EventHandle size missmatch");

        assert_eq!(std::mem::size_of::<Action>(), 40, "Action size missmatch");

        assert_eq!(std::mem::size_of::<MessagePtr>(), 24, "MessagePtr size missmatch");
        assert_eq!(std::mem::size_of::<MessageValue>(), 40, "MessageValue size missmatch");
        assert_eq!(std::mem::size_of::<MessageType>(), 1, "MessageType size missmatch");
        assert_eq!(std::mem::size_of::<Message>(), 48, "Message size missmatch");

        assert_eq!(std::mem::size_of::<PropertyValue>(), 8, "PropertyValue size missmatch");
        assert_eq!(std::mem::size_of::<PropertyType>(), 1, "PropertyType size missmatch");
        assert_eq!(std::mem::size_of::<Property>(), 16, "Property size missmatch");

        assert_eq!(std::mem::size_of::<PluginDescription>(), 32, "PluginDescription size missmatch");

        assert_eq!(std::mem::size_of::<DataStoreReturnCode>(), 1, "DataStoreReturnCode size missmatch");

        assert_eq!(std::mem::size_of::<ReturnValue_u64>(), 16, "ReturnValue_u64 size missmatch");
        assert_eq!(std::mem::size_of::<ReturnValue_PropertyHandle>(), 24, "ReturnValue_PropertyHandle size missmatch");
        assert_eq!(std::mem::size_of::<ReturnValue_EventHandle>(), 24, "ReturnValue_EventHandle size missmatch");
        assert_eq!(std::mem::size_of::<ReturnValue_ActionHandle>(), 24, "ReturnValue_ActionHandle size missmatch");
        assert_eq!(std::mem::size_of::<ReturnValue_Property>(), 24, "ReturnValue_Property size missmatch");

        assert_eq!(std::mem::size_of::<PluginSettingsLoadFail>(), 8, "PluginSettingsLoadFail size missmatch");
        assert_eq!(std::mem::size_of::<PluginSettingsLoadReturn>(), 16, "PluginSettingsLoadReturn size missmatch");
    }

}
