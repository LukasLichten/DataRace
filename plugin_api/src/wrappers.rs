use std::{fmt::Display, num::TryFromIntError, os::raw::c_void};
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

// User is required locking when acting with these, but forcing them to make wrappers is just silly
unsafe impl Sync for PluginHandle {}
unsafe impl Send for PluginHandle {}

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

/// The handle for an Event, for creating, triggering, subscribing, unsubscribing and identifying
#[derive(Debug, Clone, Copy)]
pub struct EventHandle {
    inner: sys::EventHandle
}

impl EventHandle {
    pub(crate) fn new(handle: sys::EventHandle) -> Self {
        EventHandle { inner: handle }
    }

    pub(crate) fn get_inner(&self) -> sys::EventHandle {
        self.inner
    }

    /// This is used by Macros in their generated Code allowing them to write down the values
    /// generated during compiletime.
    /// This does not serve any further purpose, and should not be used by you
    #[inline]
    pub const unsafe fn from_values(plugin_hash: u64, event_hash: u64) -> Self {
        EventHandle { inner: sys::EventHandle { plugin: plugin_hash, event: event_hash } }
    }
}

impl PartialEq for EventHandle {
    fn eq(&self, other: &Self) -> bool {
        self.get_inner().plugin == other.get_inner().plugin &&
            self.get_inner().event == other.get_inner().event
    }
}

/// Handle to access values of a Property that is an array.
///
/// These handles are long lived, and will receive changes to values contained.
/// However if the Property is resized or the type changed, then a new handle is required to be
/// optained.
#[derive(Debug)]
pub struct ArrayHandle {
    ptr: *mut sys::ArrayValueHandle
}

// All data within the ArrayHandle (which is effectivly a Arc wrapper for it) is synced
unsafe impl Sync for ArrayHandle {}
unsafe impl Send for ArrayHandle {}

impl ArrayHandle {

    /// Creates a new ArrayHandle with the size defined in `size`,
    /// and type (and inital value) of `value`.
    ///
    /// The type and size can not be changed without creating a new array.
    ///
    /// The only permissable types are Int, Float, Bool, String and Duration.
    /// None and Array will cause this function to fail, no array to be created, and return None.
    pub fn new(handle: &PluginHandle, value: Property, size: usize) -> Option<Self> {
        let ptr = unsafe {
            sys::create_array(handle.ptr, size, value.to_c())
        };

        if !ptr.is_null() {
            Some(ArrayHandle { ptr })
        } else {
            None
        }
    }

    /// Retrieves a value at a certain index.
    ///
    /// None if the index is out of bounds.
    #[inline]
    pub fn get(&self, index: usize) -> Option<Property> {
        let raw_value = unsafe {
            sys::get_array_value(self.ptr, index)
        };

        if raw_value.sort == sys::PropertyType_None {
            None
        } else {
            Some(Property::new(raw_value))
        }
    }

    /// Sets a value at a certain index
    ///
    /// It will fail if you:
    /// - Lack write permission (NotAuthenticated)
    /// - Out of Bounds (DoesNotExist)
    /// - Different Datatype then used in the array (TypeMissmatch)
    #[inline]
    pub fn set(&self, handle: &PluginHandle, index: usize, value: Property) -> DataStoreReturnCode {
        let res = DataStoreReturnCode::from(unsafe {
            sys::set_array_value(handle.ptr, self.ptr, index, value.to_c())
        });

        res
    }

    /// Returns the size of the array
    #[inline]
    pub fn len(&self) -> usize {
        unsafe {
            sys::get_array_length(self.ptr)
        }
    }

    /// Creates a Iterator for this array
    pub fn iter<'a>(&'a self) -> ArrayIterator<'a> {
        ArrayIterator { handle: self, index: 0 }
    }
}

impl Drop for ArrayHandle {
    fn drop(&mut self) {
        unsafe {
            // We could check for null pointers (aka values that got parse to_c()), but libdatarace does that too
            sys::drop_array_handle(self.ptr);
        }
    }
}

impl Clone for ArrayHandle {
    fn clone(&self) -> Self {
        let ptr = unsafe {
            sys::clone_array_handle(self.ptr)
        };

        Self { ptr }
    }
}

/// Iterator over the ArrayHandle
pub struct ArrayIterator<'a> {
    handle: &'a ArrayHandle,
    index: usize
}

impl Iterator for ArrayIterator<'_> {
    type Item = Property;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.handle.get(self.index);

        self.index += 1;

        item
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
    Duration(i64),
    Array(ArrayHandle)
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
            },
            sys::PropertyType_Array => {
                let ptr = unsafe {
                    prop.value.arr
                };

                Property::Array(ArrayHandle { ptr })
            },
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
                // We need to insure our string does not contain null bytes, so we split on the
                // first null byte and use that substring
                let filtered = s.split(char::from(0)).next().expect("failed to convert string into CString");

                let c_str = CString::new(filtered).expect("failed to convert string into CString").into_raw();
                sys::Property { sort: sys::PropertyType_Str, value: sys::PropertyValue { str_: c_str } }
            },
            Property::Duration(d) => sys::Property { sort: sys::PropertyType_Duration, value: sys::PropertyValue { dur: d } },
            Property::Array(mut arr) => {
                let v = sys::Property { sort: sys::PropertyType_Array, value: sys::PropertyValue { arr: arr.ptr } };

                // We replace the pointer with a null pointer,
                // as due to to_c() consuming self the destructor is called on the ArrayHandle
                // wrapper, resulting in the drop function calling drop_array_handle, when it
                // already transfered ownership to the new owner, causing a double free.
                //
                // However changing the pointer will cause it to attempt to deallocate a null
                // pointer, doing nothing besides cleaning up the wrapper
                arr.ptr = std::ptr::null_mut();
                drop(arr);


                v
            }

        }
    }

    /// This function will panic if a duration of more then 584,942 years is requested
    /// Negative durations are supported through the negative flag
    pub fn from_duration(dur: std::time::Duration, negative: bool) -> Self {
        let mut t:i64 = dur.as_micros().try_into().expect("Why in the ever loving world did you need more then 580k years?");
        if negative {
            t *= -1;
        }

        Property::Duration(t)
    }

    /// Negative durations are supported
    pub fn from_micros(dur: i64) -> Self {
        Property::Duration(dur)
    }

    /// This function will panic if a duration of more then 584,942 years is requested
    /// Negative durations are supported
    pub fn from_millis(dur: i64) -> Self {
        let val = dur.checked_mul(1000).expect("Why in the ever loving world did you need more then 580k years?");

        Property::Duration(val)
    }

    /// For precision it is recommended to use an integer with `from_millis` or `from_micros` due
    /// to floating point error.  
    /// This function will overflow if more then 584,942 years is requested.
    /// Negative durations are supported
    pub fn from_sec(dur: f64) -> Self {
        let dur = dur * 1000.0 * 1000.0;
        let val:i64 = dur as i64;

        Property::Duration(val)
    }

    /// If this is a Property::Duration this will convert the contained time into a Rust Duration.  
    /// As Duration does not support negative time stamps the boolean indicates negativity.
    pub fn to_duration(&self) -> Option<(std::time::Duration, bool)> {
        if let Property::Duration(t) = self {
            let (neg, val) = if *t < 0 {
                (true, t * -1)
            } else {
                (false, t.clone())
            };

            let dur = std::time::Duration::from_micros(val as u64);
            Some((dur, neg))
        } else {
            None
        }
    }

    /// Uses `ToString` to convert text types into a Property.
    pub fn from_string<T>(value: T) -> Self where T: ToString {
        Property::Str(value.to_string())
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
            Property::Duration(d) => format!("{}us", d.to_string()),
            Property::Array(arr) => {
                let mut ouput = "[".to_string();

                let mut iter = arr.iter();
                while let Some(item) = iter.next() {
                    if let Property::Str(text) = item {
                        ouput = format!("{}\"{}\", ", ouput, text)
                    } else {
                        ouput = format!("{}{}, ", ouput, item.to_string())
                    }
                }

                if let Some(pre) = ouput.strip_suffix(", ") {
                    format!("{}]", pre)
                } else {
                    format!("{}]", ouput)
                }
            }
        }
    }
}

impl From<i64> for Property {
    fn from(value: i64) -> Self {
        Property::Int(value)
    }
}

impl From<i32> for Property {
    fn from(value: i32) -> Self {
        Property::Int(value.into())
    }
}

impl From<u32> for Property {
    fn from(value: u32) -> Self {
        Property::Int(value.into())
    }
}

impl From<i16> for Property {
    fn from(value: i16) -> Self {
        Property::Int(value.into())
    }
}

impl From<u16> for Property {
    fn from(value: u16) -> Self {
        Property::Int(value.into())
    }
}

impl From<i8> for Property {
    fn from(value: i8) -> Self {
        Property::Int(value.into())
    }
}

impl From<u8> for Property {
    fn from(value: u8) -> Self {
        Property::Int(value.into())
    }
}

impl TryFrom<usize> for Property {
    type Error = TryFromIntError;

    fn try_from(value: usize) -> Result<Self, Self::Error> {
        Ok(Property::Int(value.try_into()?))
    }
}

impl TryFrom<isize> for Property {
    type Error = TryFromIntError;

    fn try_from(value: isize) -> Result<Self, Self::Error> {
        Ok(Property::Int(value.try_into()?))
    }
}

impl TryFrom<u64> for Property {
    type Error = TryFromIntError;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        Ok(Property::Int(value.try_into()?))
    }
}

impl From<f32> for Property {
    fn from(value: f32) -> Self {
        Property::Float(value.into())
    }
}

impl From<f64> for Property {
    fn from(value: f64) -> Self {
        Property::Float(value)
    }
}

impl From<bool> for Property {
    fn from(value: bool) -> Self {
        Property::Bool(value)
    }
}

impl From<std::time::Duration> for Property {
    /// This function will panic if a duration of more then 584,942 years is requested
    /// If you want to define a negative duration, use `from_duration()`
    fn from(value: std::time::Duration) -> Self {
        Property::Duration(value.as_micros().try_into().expect("Why in the ever loving world did you need more then 580k years?"))
    }
}

impl From<String> for Property {
    fn from(value: String) -> Self {
        Property::Str(value)
    }
}

impl From<&str> for Property {
    fn from(value: &str) -> Self {
        Property::Str(value.to_string())
    }
}

impl From<ArrayHandle> for Property {
    fn from(value: ArrayHandle) -> Self {
        Property::Array(value)
    }
}

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

impl DataStoreReturnCode {
    pub fn to_result(self) -> Result<(), DataStoreReturnCode> {
        match self {
            DataStoreReturnCode::Ok => Ok(()),
            e => Err(e)
        }
    }

    pub fn is_ok(&self) -> bool {
        match self {
            DataStoreReturnCode::Ok => true,
            _ => false
        }
    }
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
    StartupFinished,
    OtherPluginStarted(u64),
    
    InternalMsg(i64),
    PluginMessagePtr{origin: u64, ptr: *mut c_void, reason: i64 },

    EventTriggered(EventHandle),
    EventUnsubscribed(EventHandle),

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
            sys::MessageType_StartupFinished => Message::StartupFinished,
            sys::MessageType_OtherPluginStarted => {
                Message::OtherPluginStarted(unsafe { value.value.plugin_id })
            },
            sys::MessageType_InternalMessage => {
                Message::InternalMsg(unsafe {
                    value.value.internal_msg
                })
            },
            sys::MessageType_PluginMessagePtr => {
                let val = unsafe { value.value.message_ptr };
                
                Message::PluginMessagePtr { origin: val.origin, ptr: val.message_ptr, reason: val.reason }
            },

            sys::MessageType_EventTriggered => {
                let val = unsafe {
                    value.value.event
                };

                Message::EventTriggered(EventHandle::new(val))
            },
            sys::MessageType_EventUnsubscribed => {
                let val = unsafe {
                    value.value.event
                };

                Message::EventUnsubscribed(EventHandle::new(val))
            },


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

/// This guard provides protection against locks from the Pluginloader,
/// the lock is released when this struct is dropped (which you should regularly do).
pub struct PluginLockGuard<'a> {
    pub(crate) handle: &'a PluginHandle
}

impl<'a> Drop for PluginLockGuard<'a> {
    fn drop(&mut self) {
        unsafe { sys::unlock_plugin(self.handle.get_ptr()) };
    }
}
