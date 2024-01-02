use libc::c_char;
use crate::utils;

/// Unique Handle of your plugin, allowing you to interact with the API
pub struct PluginHandle {
    pub(crate) name: String,
    pub(crate) datastore: &'static crate::datastore::DataStore,
    pub(crate) token: crate::datastore::Token
}

/// Return codes from operations like create_property, etc.
#[derive(PartialEq)]
#[repr(u8)]
pub enum DataStoreReturnCode {
    Ok = 0,
    NotAuthenticated = 1,
    AlreadyExists = 2,
    DoesNotExist = 3,
    OutdatedPropertyHandle = 4,
    TypeMissmatch = 5,
    DataCorrupted = 10

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
/// These handles can be from time to time invalidated if a property seizes to exist
#[repr(C)]
pub struct PropertyHandle {
    pub index: usize,
    pub hash: u64
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

impl Default for PropertyHandle {
    fn default() -> Self {
        PropertyHandle { index: 0, hash: 0 }    
    }
}

impl Default for Property {
    fn default() -> Self {
        Property { sort: PropertyType::None, value: PropertyValue { integer: 0 } }
    }
}
