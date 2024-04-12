use std::{ffi::CString, mem::ManuallyDrop, sync::Arc};

use libc::c_char;
use crate::utils; 
use hashbrown::HashMap;

/// Unique Handle of your plugin, allowing you to interact with the API
pub struct PluginHandle {
    pub(crate) name: String,
    pub(crate) datastore: &'static tokio::sync::RwLock<crate::datastore::DataStore>,
    pub(crate) id: u64,
    pub(crate) subscriptions: HashMap<PropertyHandle, Arc<utils::ValueContainer>>,
    pub(crate) properties: HashMap<PropertyHandle, Arc<utils::ValueContainer>>,
}

impl PluginHandle {
    pub(crate) fn new(name: String, id: u64, datastore: &'static tokio::sync::RwLock<crate::datastore::DataStore>) -> PluginHandle {
        PluginHandle {
            name,
            datastore,
            id,
            subscriptions: HashMap::default(),
            properties: HashMap::default()
        }
    }
}

/// Return codes from operations like create_property, etc.
#[derive(PartialEq, Debug)]
#[repr(u8)]
pub enum DataStoreReturnCode {
    Ok = 0,
    NotAuthenticated = 1,
    AlreadyExists = 2,
    DoesNotExist = 3,
    OutdatedPropertyHandle = 4, //  TODO: Remove handle
    TypeMissmatch = 5,
    NotImplemented = 6,
    ParameterCorrupted = 10, 
    DataCorrupted = 11

}

/// A Descriptor for the plugin, used to aquire meta data (name/version),
/// but also to check compatibility (api_version and id)
/// api_version and id should be values generated at compiletime
#[repr(C)]
pub struct PluginDescription {
    pub name: *mut c_char,
    pub id: u64,
    pub version: [u16;3],
    pub api_version: u64,
    
}

/// Return Value for an API function
/// Only if the ReturnCode is OK (aka 0), then the value is defined
/// If the ReturnCode is not 0, then the value is still alocated with a default zero value
#[repr(C)]
pub struct ReturnValue<T> {
    pub code: DataStoreReturnCode,
    pub value: T
}

/// A Handle that serves for easy access to getting and updating properties
/// These handles can (and should be where possible) generated at compile time
#[repr(C)]
#[derive(Clone,Copy,PartialEq,Hash,Debug)]
pub struct PropertyHandle {
    pub plugin: u64,
    pub property: u64
}

impl Default for PropertyHandle {
    fn default() -> Self {
        PropertyHandle { plugin: 0, property: 0 }
    }
}

impl PropertyHandle {
    pub(crate) fn new(str: &str) -> Option<Self> {
        let str = str.trim();
        let mut split = str.splitn(2, '.');

        let plugin_name = split.next()?;
        let prop_name = split.next()?;

        Some(Self { plugin: utils::generate_plugin_name_hash(plugin_name)?, property: utils::generate_property_name_hash(prop_name)? })
    }
}

/// The Type and Value of a Property
#[repr(C)]
pub struct Property {
    pub sort: PropertyType,
    pub value: PropertyValue
}

/// The type of this Property
#[repr(u8)]
pub enum PropertyType {
    None = 0,
    Int = 1,
    Float = 2,
    Boolean = 3,
    Str = 4,
    Duration = 5
}

/// This is a union, only one type is actually contained (read the PropertyType value first)
/// integer is a 64bit signed integer
/// decimal is a double precision (64bit) floating point number
/// boolean is a Boolean
/// str is a pointer to a null terminating String
/// dur is a Duration in micro seconds (1s = 1,000millis = 1,000,000 micros), signed
#[repr(C)]
pub union PropertyValue {
    pub integer: i64,
    pub decimal: f64,
    pub boolean: bool,
    // this is the reason to not support clone
    pub str: *mut c_char,
    pub dur: i64
}

impl<T> ReturnValue<T> where T: Default {
    pub fn new_from_error(code: DataStoreReturnCode) -> Self {
        ReturnValue { code, value: T::default() }
    }
}

impl<T> From<Result<T,DataStoreReturnCode>> for ReturnValue<T> where T: Default {
    fn from(value: Result<T,DataStoreReturnCode>) -> Self {
        match value {
            Ok(val) => ReturnValue { code: DataStoreReturnCode::Ok, value: val },
            Err(e) => ReturnValue { code: e, value: T::default() }
        }
    }
}

impl Default for Property {
    fn default() -> Self {
        Property { sort: PropertyType::None, value: PropertyValue { integer: 0 } }
    }
}

impl TryFrom<crate::utils::Value> for Property {
    type Error = DataStoreReturnCode;

    fn try_from(value: utils::Value) -> Result<Self, Self::Error> {
        Ok(match value {
            utils::Value::None => Property::default(),
            utils::Value::Int(i) => Property { sort: PropertyType::Int, value: PropertyValue { integer: i } },
            utils::Value::Float(f) => Property { sort: PropertyType::Float, value: PropertyValue { decimal: f64::from_be_bytes(f.to_be_bytes()) } },
            utils::Value::Bool(b) => Property { sort: PropertyType::Boolean, value: PropertyValue { boolean: b } },
            utils::Value::Str(s) => {
                if let Ok(val) = CString::new(s.as_str().to_string()) {
                    let ptr = val.into_raw();
                    Property { sort: PropertyType::Str, value: PropertyValue { str: ptr } }
                } else {
                    return Err(DataStoreReturnCode::DataCorrupted);
                }
            },
            utils::Value::Dur(d) => Property { sort: PropertyType::Duration, value: PropertyValue { dur: d }}
        })
    }
}

#[repr(C)]
pub struct Message {
    pub sort: MessageType,
    pub value: MessageValue
}

#[repr(u8)]
pub enum MessageType {
    Update = 0,
    Removed = 1
}

#[repr(C)]
pub union MessageValue {
    pub removed_property: PropertyHandle,
    pub update: ManuallyDrop<UpdateValue> 
}

#[repr(C)]
pub struct UpdateValue {
    pub handle: PropertyHandle,
    pub value: Property
}

impl TryFrom<crate::pluginloader::Message> for Message {
    type Error = ();

    fn try_from(value: crate::pluginloader::Message) -> Result<Self, Self::Error> {
        Ok(match value {
            crate::pluginloader::Message::Update(handle, value) => {
                if let Ok(value) = Property::try_from(value) {
                    Message { sort: MessageType::Update, value: MessageValue { update: ManuallyDrop::new(UpdateValue { handle, value } )  } }
                } else {
                    return Err(());
                }
            },
            crate::pluginloader::Message::Removed(handle) => {
                Message { sort: MessageType::Removed, value: MessageValue { removed_property: handle }}

            },
            _ => return Err(())
        })
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        match self.sort {
            MessageType::Update => unsafe {
                ManuallyDrop::drop(&mut self.value.update);
            },
            _ => ()
        }
    }
}

impl Default for UpdateValue {
    fn default() -> Self {
        UpdateValue { handle: PropertyHandle::default(), value: Property::default() }
    }
}
