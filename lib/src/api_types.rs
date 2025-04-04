use std::{sync::Arc, usize};

use libc::c_char;
use log::error;
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
    lock: std::sync::atomic::AtomicU32,
    pub(crate) event_channel: kanal::Sender<crate::events::EventMessage>
}

impl PluginHandle {
    pub(crate) fn new(name: String,
        id: u64,
        datastore: &'static tokio::sync::RwLock<crate::datastore::DataStore>,
        sender: kanal::Sender<crate::pluginloader::LoaderMessage>,
        free_string: extern "C" fn(ptr: *mut libc::c_char),
        version: [u16;3],
        event_channel: kanal::Sender<crate::events::EventMessage>
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
            state_ptr: std::ptr::null_mut(),
            event_channel
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
        self.lock.store(0, std::sync::atomic::Ordering::Release);
        atomic_wait::wake_one(&self.lock);
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
///
/// Only if the ReturnCode is OK (aka 0), then the value is defined.
/// If the ReturnCode is not 0, then the value is still alocated with a default zero value
#[repr(C)]
pub struct ReturnValue<T> {
    pub code: DataStoreReturnCode,
    pub value: T
}

/// A Handle that serves for easy access to getting and updating properties
///
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

/// A Handle that represents a event
///
/// They work similar to [`PropertyHandle`], except they represent Events (duh).
/// Used to create, subscribe and identify received Events.
///
/// These handles can (and should be where possible) generated at compile time
#[repr(C)]
#[derive(Clone,Copy,PartialEq,Eq,Hash,Debug)]
pub struct EventHandle {
    pub plugin: u64,
    pub event: u64
}

impl Default for EventHandle {
    fn default() -> Self {
        EventHandle { plugin: 0, event: 0 }
    }
}

impl EventHandle {
    pub(crate) fn new(str: &str) -> Option<Self> {
        let str = str.trim();
        let mut split = str.splitn(2, '.');

        let plugin_name = split.next()?;
        let prop_name = split.next()?;

        Some(Self { plugin: utils::generate_plugin_name_hash(plugin_name)?, event: utils::generate_event_name_hash(prop_name)? })
    }
}

/// The Type and Value of a Property
#[repr(C)]
pub struct Property {
    pub sort: PropertyType,
    pub value: PropertyValue
}

impl Property {
    /// This is used internally to deallocate a Property, mostly correctly,
    /// when it is not read in for any purpose
    pub(crate) unsafe fn dealloc(self) {
        match self.sort {
            PropertyType::None => (),
            PropertyType::Int => (),
            PropertyType::Float => (),
            PropertyType::Boolean => (),
            PropertyType::Duration => (),

            PropertyType::Array => {
                unsafe { 
                    let ptr = self.value.arr;
                    if !ptr.is_null() {
                        ptr.drop_in_place();
                    }
                };
            },
            PropertyType::Str => {
                let ptr = unsafe {
                    self.value.str
                };

                let str = unsafe { std::ffi::CString::from_raw(ptr) };
                drop(str);
            }
        }
    }
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
    Duration = 5,

    Array = 10
}

/// This is a union, only one type is actually contained (read the PropertyType value first)
/// integer is a 64bit signed integer
/// decimal is a double precision (64bit) floating point number
/// boolean is a Boolean
/// str is a pointer to a null terminating String
/// dur is a Duration in micro seconds (1s = 1,000millis = 1,000,000 micros), signed
/// arr is a pointer to a ArrayValue
#[repr(C)]
pub union PropertyValue {
    pub integer: i64,
    pub decimal: f64,
    pub boolean: bool,
    // this is the reason to not support clone
    pub str: *mut c_char,
    pub dur: i64,
    pub arr: *mut ArrayValueHandle,
}

/// Handle to the array contained in a property.
///
/// These are long lived references, values retrieved are always up to date, so you can use them
/// indefintly.
/// Except they have a fixed size and type, so if you need to change either you need to create new array,
/// and retrieve a new handle.
///
/// It is important to call `drop_array_handle` on a handle when it goes out of scope.
/// You can produce a second handle (that points to the same underlying array) use `clone_array_handle`.
pub struct ArrayValueHandle {
    pub(crate) arr: Arc<utils::ArrayValueContainer>,
    pub(crate) allow_modify: bool
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
    StartupFinished = 0,
    OtherPluginStarted = 1,
    InternalMessage = 2,
    PluginMessagePtr = 5,

    EventTriggered = 6,
    EventUnsubscribed = 7,
    ActionRecv = 8,
    ActionCallback = 9,

    // Update = 0,
    // Removed = 1,
    Lock = 10,
    Unlock = 11,
    Shutdown = 20,
}

#[repr(C)]
pub union MessageValue {
    pub plugin_id: u64,
    pub internal_msg: i64,
    pub message_ptr: MessagePtr,
    pub flag: bool,
    pub event: EventHandle,
    pub action: Action
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MessagePtr {
    pub origin: u64,
    pub message_ptr: *mut libc::c_void,
    pub reason: i64
}

/// Action serves for both Action and ActionCallback events.
///
/// As an ActionRecv, the action value is the action hash from the ActionHandle.
/// For ActionCallback, the action value is 0 for success, others are plugin defined errors.
///
/// Params is an Array with the size param_count. When param_count == 0, then params is null.
///
/// id is a unique id for this specific action event, used to trigger the callback (and to identify it).
/// Ids are unique across all plugins, unilaterally climbing, and starts at 0 when DataRace is
/// started.  
/// The value could overflow, but similar to Property::Duration, even at 1 action per
/// microsecond (unlikely to achieve such throughput), then it would overflow in 584,942 years.
/// When actions are triggered in parallel you may receive actions with ids out of order.
///
/// The origin is the plugin hash of the caller.
/// Useful for ActionRecv, but for ActionCallback you should know who you called.
///
/// You should send a callback for any ActionRecv, even when the action attempted to call is undefined for you.
/// Never send a callback for an ActionCallback, as this reuses the id, and could confuse behavior.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Action {
    pub action: u64,
    pub params: *mut Property,
    pub param_count: usize,
    pub id: u64,
    pub origin: u64
}

// Need due to being send through loader Message
unsafe impl Send for Action {}

impl Action {
    pub(crate) fn new(sender_plugin_id: u64, action: u64, params: *mut Property, param_count: usize) -> Self {
        let (params, param_count) = match (params.is_null(), param_count) {
            // Standard case for parameterless calling:
            (true, 0) => (std::ptr::null_mut(), 0),
            // Pointer to somewhere but 0 size? We will leak whatever this is pointing 
            // (if anything at all) and continue with (null, 0)
            (false, 0) => { 
                error!("Action parameters malformed: param_count is 0 but params is not a null pointer. Ignoring params");
                (std::ptr::null_mut(), 0)
            },
            // Nullpointer, paramtercount above 0, ignore parameter count
            (true, _) => {
                error!("Action parameters malformed: params is a null pointer, but param_counter is not 0. Ignoring param_counter");
                (std::ptr::null_mut(), 0)
            },
            // No way of knowing if these are allocated, or actually the size... just hope
            (false, _) => (params, param_count)
        };

        Self { action, params, param_count, id: u64::MAX, origin: sender_plugin_id }
    }

    /// Consumes this Action and deallocates it's params array
    pub(crate) unsafe fn dealloc(mut self) {
        // we have to do this with a custom function, as Copy trait conflicts with Drop, and we
        // need Copy for the Union.
        
        // Insurance that params is correctly formated
        self = Self::new(self.origin, self.action, self.params, self.param_count);
        
        if self.param_count == 0 {
            return;
        }

        unsafe {
            let para = Vec::<Property>::from_raw_parts(self.params, self.param_count, self.param_count);
            
            for item in para {
                item.dealloc();
            }
        }
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ActionHandle {
    pub plugin: u64,
    pub action: u64
}

impl ActionHandle {
    pub(crate) fn new(name: &str) -> Option<ActionHandle> {
        let str = name.trim();
        let mut split = str.splitn(2, '.');

        let plugin_name = split.next()?;
        let prop_name = split.next()?;

        Some(Self { plugin: utils::generate_plugin_name_hash(plugin_name)?, action: utils::generate_action_name_hash(prop_name)? })
    }
}

impl Default for ActionHandle {
    fn default() -> Self {
        ActionHandle { plugin: 0, action: 0 }
    }
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
