use libc::c_char; 
use std::{ffi::{CStr, CString}, sync::Arc};

use crate::{Property, PropertyType};

/// Simple way to aquire a String for a null terminating c_char ptr
/// We do not optain ownership of the String, the owner has to deallocate it
///
/// But does to_string clone the bytes? If you need to store this data longer then the API call
/// then clone it
pub fn get_string(ptr: *mut c_char) -> Option<String> {
    Some(unsafe {
        let c_str = CStr::from_ptr(ptr);

        if let Ok(it) = c_str.to_str() {
            it
        } else {
            return None;
        }
    }.to_string())
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    None,
    Int(i64),
    Float(u64),
    Bool(bool),
    Str(Arc<String>),
    Dur(i64)
}

impl Value {
    pub fn new(prop: Property) -> Value {
        match prop.sort {
            PropertyType::None => Value::None,
            PropertyType::Int => {
                let val = unsafe {
                    prop.value.integer
                };
                Value::Int(val)
            },
            PropertyType::Float => {
                let val = unsafe {
                    prop.value.decimal
                };
                Value::Float(u64::from_be_bytes(val.to_be_bytes()))
            },
            PropertyType::Boolean => {
                let val = unsafe {
                    prop.value.boolean
                };
                Value::Bool(val)
            },
            PropertyType::Str => {
                let ptr = unsafe {
                    prop.value.str
                };
                if let Some(val) = get_string(ptr) {
                    // I am not 100% sure we are properly disposing of the original cstring
                    // Does to_string clone the data?
                    // Does Arc clone the data?
                    // we just call clone here so we can "safely" drop the Cstring
                    let re = Value::Str(Arc::new(val.clone()));

                    unsafe {
                        // Deallocating resources a different allocater has allocated is ill
                        // advised, but we had been given this object, we need to clean up too
                        drop(CString::from_raw(ptr));
                    }
                    re
                } else {
                    Value::None
                }
            },
            PropertyType::Duration => {
                let val = unsafe {
                    prop.value.dur
                };
                Value::Dur(val)
            }
        }

        // Rust should deallocate the Property Object as we got ownership in the call, and passed
        // it into this function
        // All values besides String (which is a pointer we have deallocated) are usize or less
    }
}

