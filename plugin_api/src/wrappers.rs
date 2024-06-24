use std::{fmt::Display, os::raw::c_void};
use crate::get_string;
use datarace_plugin_api_sys as sys;
use std::ffi::CString;


/// The handle for this plugin passed through into this plugin from the API
/// Used for call to the Plugin API
#[derive(Debug, Clone)]
pub struct PluginHandle {
    ptr: *mut crate::reexport::PluginHandle,
}

impl PluginHandle {
    pub unsafe fn new(ptr: *mut crate::reexport::PluginHandle) -> PluginHandle {
        PluginHandle { ptr }
    }

    #[inline]
    pub(crate) fn get_ptr(&self) -> *mut crate::reexport::PluginHandle {
        self.ptr
    }

    /// Raw access to the state pointer.
    /// If you want a save and more convient way check out macros::get_state! for more info
    pub unsafe fn get_state_ptr(&self) -> *mut c_void {
        sys::get_state(self.ptr)
    }

    /// Raw access to the state pointer.
    /// If you want to use it in a more convient way, check out macros::save_state_now! and
    /// macros::plugin_init for more info.
    pub unsafe fn store_state_ptr_now(&self, ptr: *mut c_void) {
        sys::save_state_now(self.ptr, ptr);
    }
}

/// A Handle for accessing Property used when writing, reading and subscribing
#[derive(Debug, Clone, Copy)]
pub struct PropertyHandle {
    inner: sys::PropertyHandle
}

impl PropertyHandle {
    pub(crate) fn new(handle: sys::PropertyHandle) -> Self {
        PropertyHandle { inner: handle }
    }

    pub(crate) fn get_inner(&self) -> sys::PropertyHandle {
        self.inner
    }

    /// This is used by Macros in their generated Code allowing them to write down the values
    /// generated during compiletime.
    /// This does not serve any further purpose, and should not be used by you
    #[inline]
    pub const unsafe fn from_values(plugin_hash: u64, property_hash: u64) -> Self {
        PropertyHandle { inner: sys::PropertyHandle { plugin: plugin_hash, property: property_hash } }
    }
}

impl PartialEq for PropertyHandle {
    fn eq(&self, other: &Self) -> bool {
        self.inner.plugin == other.inner.plugin && self.inner.property == other.inner.property
    }
}

/// Value of a Property
/// This type is used for setting and getting Values
///
/// Note:
/// Duration is messured in micro seconds (1s = 1,000 ms = 1,000,000 us), and is signed
/// So, while std::time::Duration does NOT support negative timespans, this DOES
#[derive(Debug, Clone)]
pub enum Property {
    None,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Duration(i64)
}

impl Property {
    pub(crate) fn new(prop: sys::Property) -> Self {
        match prop.sort {
            sys::PropertyType_None => Property::None,
            sys::PropertyType_Int => {
                let val = unsafe {
                    prop.value.integer
                };

                Property::Int(val)
            },
            sys::PropertyType_Float => {
                let val = unsafe {
                    prop.value.decimal
                };

                Property::Float(val)
            },
            sys::PropertyType_Boolean => {
                let val = unsafe {
                    prop.value.boolean
                };

                Property::Bool(val)
            },
            sys::PropertyType_Str => {
                let ptr = unsafe {
                    prop.value.str_
                };
                if let Some(val) = get_string(ptr) {
                    // I am not 100% sure we are properly disposing of the original cstring
                    // Does to_string clone the data?
                    // we just call clone here so we can "safely" drop the Cstring
                    let re = Property::Str(val.clone());

                    unsafe {
                        sys::deallocate_string(ptr);
                    }
                    re
                } else {
                    Property::None
                }
            },
            sys::PropertyType_Duration => {
                let val = unsafe {
                    prop.value.dur
                };

                Property::Duration(val)
            }
            _ => Property::None
        }
    }

    pub(crate) fn to_c(self) -> sys::Property {
        match self {
            Property::None => sys::Property { sort: sys::PropertyType_None, value: sys::PropertyValue { integer: 0 } },
            Property::Int(i) => sys::Property { sort: sys::PropertyType_Int, value: sys::PropertyValue { integer: i } },
            Property::Float(f) => sys::Property { sort: sys::PropertyType_Float, value: sys::PropertyValue { decimal: f } },
            Property::Bool(b) => sys::Property { sort: sys::PropertyType_Boolean, value: sys::PropertyValue { boolean: b } },
            Property::Str(s) => {
                let c_str = CString::new(s).unwrap().into_raw();
                sys::Property { sort: sys::PropertyType_Str, value: sys::PropertyValue { str_: c_str } }
            },
            Property::Duration(d) => sys::Property { sort: sys::PropertyType_Duration, value: sys::PropertyValue { dur: d } },

        }
    }
}

impl ToString for Property {
    fn to_string(&self) -> String {
        match self {
            Property::None => "NONE".to_string(),
            Property::Int(i) => i.to_string(),
            Property::Float(f) => f.to_string(),
            Property::Bool(b) => b.to_string(),
            Property::Str(s) => s.clone(),
            Property::Duration(d) => format!("{}us", d.to_string())
        }
    }
}

// TODO From<X> function for Property

/// Serve as status codes for api calls
#[derive(Debug, PartialEq)]
pub enum DataStoreReturnCode {
    Ok = 0,
    NotAuthenticated = 1,
    AlreadyExists = 2,
    DoesNotExist = 3,
    TypeMissmatch = 5,
    NotImplemented = 6,
    ParameterCorrcupted = 10,
    DataCorrupted = 11,
    Unknown = 255

}

impl From<sys::DataStoreReturnCode> for DataStoreReturnCode {
    fn from(value: sys::DataStoreReturnCode) -> Self {
        match value {
            sys::DataStoreReturnCode_Ok => DataStoreReturnCode::Ok,
            sys::DataStoreReturnCode_NotAuthenticated => DataStoreReturnCode::NotAuthenticated,
            sys::DataStoreReturnCode_AlreadyExists => DataStoreReturnCode::AlreadyExists,
            sys::DataStoreReturnCode_DoesNotExist => DataStoreReturnCode::DoesNotExist,
            sys::DataStoreReturnCode_TypeMissmatch => DataStoreReturnCode::TypeMissmatch,
            sys::DataStoreReturnCode_NotImplemented => DataStoreReturnCode::NotImplemented,
            sys::DataStoreReturnCode_ParameterCorrupted => DataStoreReturnCode::ParameterCorrcupted,
            sys::DataStoreReturnCode_DataCorrupted => DataStoreReturnCode::DataCorrupted,
            _ => DataStoreReturnCode::Unknown
        }
    }
}

impl Display for DataStoreReturnCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.pad(match self {
            DataStoreReturnCode::Ok => "Everything went correct, value ready",
            DataStoreReturnCode::NotAuthenticated => "Action denied: Lack of authority",
            DataStoreReturnCode::AlreadyExists => "Action failed: An Item with this designation already exists",
            DataStoreReturnCode::DoesNotExist => "Action failed: Can not access item that does not exist",
            DataStoreReturnCode::TypeMissmatch => "Action failed: You can only use the same type for updates as you created it with (or use change_property_type)",
            DataStoreReturnCode::NotImplemented => "Action denied: This function has to still be implemented",
            DataStoreReturnCode::ParameterCorrcupted => "Action failed: Parameters are inproperly formated or otherwise incorrect",
            DataStoreReturnCode::DataCorrupted => "Error: Unable to parse input Data. This indicates a corrupted PluginHandle or Datastore, which are non recoverable",
            DataStoreReturnCode::Unknown => "Action failed for an unknown reason. Plugin is too out of date to know this message, possibly the reason for the Error"
        })
    }
}


pub enum Message {
    Lock,
    Unlock,
    Shutdown,

    // Update(PropertyHandle, Property),
    // Remove(PropertyHandle),


    Unknown
}

impl From<sys::Message> for Message {
    fn from(value: sys::Message) -> Self {
        match value.sort {
            sys::MessageType_Shutdown => Message::Shutdown,
            sys::MessageType_Lock => Message::Lock,
            sys::MessageType_Unlock => Message::Unlock,
            // sys::MessageType_Update => {
            //     unsafe {
            //         let val = value.value.update;
            //         
            //         Message::Update(PropertyHandle::new(val.handle), Property::new(val.value))
            //     }
            // },
            // sys::MessageType_Removed => {
            //     unsafe {
            //         let val = value.value.removed_property;
            //         
            //         Message::Removed(PropertyHandle::new(val))
            //     }
            // },
            _ => Message::Unknown
        }
    }
}

impl Message {
    #[allow(dead_code)]
    pub(crate) fn to_c(self) -> sys::Message {
        todo!("Implement to c for Message for reenqueueing...");
    }
}
