use std::mem::ManuallyDrop;

use libc::c_char;
use crate::utils; 
use hashbrown::HashMap;

/// Unique Handle of your plugin, allowing you to interact with the API
pub struct PluginHandle {
    pub(crate) name: String,
    pub(crate) datastore: &'static tokio::sync::RwLock<crate::datastore::DataStore>,
    pub(crate) id: u64,
    pub(crate) subscriptions: HashMap<PropertyHandle, utils::ValueContainer>,
    pub(crate) properties: HashMap<u64, utils::PropertyContainer>,
    pub(crate) sender: kanal::Sender<crate::pluginloader::LoaderMessage>,
    pub(crate) version: [u16;3],
    pub(crate) state_ptr: *mut libc::c_void,
    free_string: extern "C" fn(ptr: *mut libc::c_char),
    lock: std::sync::atomic::AtomicU32
}

impl PluginHandle {
    pub(crate) fn new(name: String,
        id: u64,
        datastore: &'static tokio::sync::RwLock<crate::datastore::DataStore>,
        sender: kanal::Sender<crate::pluginloader::LoaderMessage>,
        free_string: extern "C" fn(ptr: *mut libc::c_char),
        version: [u16;3]
    ) -> PluginHandle {
        PluginHandle {
            name,
            datastore,
            id,
            subscriptions: HashMap::default(),
            properties: HashMap::default(),
            free_string,
            sender,
            version,
            lock: std::sync::atomic::AtomicU32::new(0),
            state_ptr: std::ptr::null_mut()
        }
    }

    pub(crate) unsafe fn free_string_ptr(&self, ptr: *mut libc::c_char) {
            (self.free_string)(ptr)
    }

    /// This locks the datastore, allowing you to take a mut of it to do modifications.
    /// What this doesn't do:
    /// - Call this plugin up to execute a lock (but will prevent Pluginloader aquiring mut)
    /// - Automatically unlock when out of scope
    pub(crate) fn lock(&self) {
        // This is a loop, as inbetween being awoken and being able to process someone could steal
        // the lock
        while self.lock.swap(1, std::sync::atomic::Ordering::AcqRel) == 1 {
            atomic_wait::wait(&self.lock, 1);
        }
    }
    
    pub(crate) fn unlock(&self) {
        self.lock.store(0, std::sync::atomic::Ordering::Release)
    }

    #[allow(dead_code)]
    pub(crate) fn is_locked(&self) -> bool {
        self.lock.load(std::sync::atomic::Ordering::Acquire) != 1
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
#[derive(Clone,Copy,PartialEq,Eq,Hash,Debug)]
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
#[derive(Debug, PartialEq)]
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

#[repr(C)]
pub struct Message {
    pub sort: MessageType,
    pub value: MessageValue
}

#[repr(u8)]
pub enum MessageType {
    // Update = 0,
    // Removed = 1,
    Lock = 10,
    Unlock = 11,
    Shutdown = 20
}

#[repr(C)]
pub union MessageValue {
    pub flag: bool,
    pub removed_property: PropertyHandle,
    pub update: ManuallyDrop<UpdateValue> 
}

#[repr(C)]
pub struct UpdateValue {
    pub handle: PropertyHandle,
    pub value: Property
}

// impl TryFrom<crate::pluginloader::LoaderMessage> for Message {
//     type Error = ();
//
//     fn try_from(value: crate::pluginloader::LoaderMessage) -> Result<Self, Self::Error> {
//         Ok(match value {
//             crate::pluginloader::LoaderMessage::Update(handle, value) => {
//                 if let Ok(value) = Property::try_from(value) {
//                     Message { sort: MessageType::Update, value: MessageValue { update: ManuallyDrop::new(UpdateValue { handle, value } )  } }
//                 } else {
//                     return Err(());
//                 }
//             },
//             crate::pluginloader::LoaderMessage::Removed(handle) => {
//                 Message { sort: MessageType::Removed, value: MessageValue { removed_property: handle }}
//
//             },
//             _ => return Err(())
//         })
//     }
// }

// impl Drop for Message {
//     fn drop(&mut self) {
//         match self.sort {
//             MessageType::Update => unsafe {
//                 ManuallyDrop::drop(&mut self.value.update);
//             },
//             _ => ()
//         }
//     }
// }

impl Default for UpdateValue {
    fn default() -> Self {
        UpdateValue { handle: PropertyHandle::default(), value: Property::default() }
    }
}
