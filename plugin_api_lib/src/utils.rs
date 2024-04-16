use libc::c_char; 
use std::{ffi::{CStr, CString}, sync::{Arc, atomic::{AtomicU64, AtomicI64, AtomicBool, Ordering}}};
use kanal::{Sender, AsyncSender, Receiver};
use highway::{HighwayHash, HighwayHasher, Key};

use tokio::sync::Mutex;

use crate::{pluginloader::LoaderMessage, PluginHandle, Property, PropertyHandle, PropertyType, PropertyValue};

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

pub fn get_message_channel() -> (Sender<LoaderMessage>, Receiver<LoaderMessage>) {
    kanal::unbounded()
}

#[derive(Debug)]
pub(crate) struct PropertyContainer {
    value: ValueContainer,
    allow_modify: bool,
    pub(crate) short_name: String,
}

impl PropertyContainer {
    pub(crate) fn new(short_name: String, value: Property, plugin_handle: &PluginHandle) -> Self {
        Self {
            short_name,
            allow_modify: true,
            value: ValueContainer::new(value, plugin_handle)
        }
    }

    pub(crate) fn update(&self, val: Property, plugin_handle: &PluginHandle) -> bool {
        if !self.allow_modify {
            // Not allowed to edit
            if val.sort == PropertyType::Str {
                unsafe {
                    plugin_handle.free_string_ptr(val.value.str);
                }
            }

            return false;
        }

        self.value.update(val, plugin_handle)
    }

    pub(crate) fn read(&self) -> Property {
        self.value.read()
    }
}

#[derive(Debug)]
pub(crate) enum ValueContainer {
    None,
    Int(AtomicI64),
    Float(AtomicU64),
    Bool(AtomicBool),
    Str(Mutex<Arc<String>>),
    Dur(AtomicI64)
}

const SAVE_ORDERING: Ordering = Ordering::Release;
const READ_ORDERING: Ordering = Ordering::Acquire;

impl ValueContainer {
    fn new(val: Property, plugin_handle: &PluginHandle) -> Self {
        let new = match val.sort {
            PropertyType::None => ValueContainer::None,
            PropertyType::Int => ValueContainer::Int(AtomicI64::default()),
            PropertyType::Float => ValueContainer::Float(AtomicU64::default()),
            PropertyType::Boolean => ValueContainer::Bool(AtomicBool::default()),
            PropertyType::Str => ValueContainer::Str(Mutex::default()),
            PropertyType::Duration => ValueContainer::Dur(AtomicI64::default())
        };
        new.update(val, plugin_handle);

        new
    }


    fn new_int(val: Value) -> ValueContainer {
        match val {
            Value::None => ValueContainer::None,
            Value::Int(i) => ValueContainer::Int(AtomicI64::new(i)),
            Value::Float(f) => ValueContainer::Float(AtomicU64::new(f)),
            Value::Bool(b) => ValueContainer::Bool(AtomicBool::new(b)),
            Value::Str(s) => ValueContainer::Str(Mutex::new(s)),
            Value::Dur(d) => ValueContainer::Dur(AtomicI64::new(d))
        }
    }

    fn update(&self, val: Property, plugin_handle: &PluginHandle) -> bool {
        match (val.sort, self) {
            (PropertyType::None, ValueContainer::None) => true,
            (PropertyType::Int, ValueContainer::Int(at)) => {
                let i = unsafe { val.value.integer };
                at.store(i, SAVE_ORDERING);
                true
            },
            (PropertyType::Float, ValueContainer::Float(at)) => {
                let f = unsafe { val.value.decimal };
                let conv = u64::from_be_bytes(f.to_be_bytes());
                at.store(conv, SAVE_ORDERING);
                true
            },
            (PropertyType::Boolean, ValueContainer::Bool(at)) => {
                let b = unsafe {
                    val.value.boolean
                };
                at.store(b, SAVE_ORDERING);
                true
            },
            (PropertyType::Str, ValueContainer::Str(mu)) => {
                let ptr = unsafe {
                    val.value.str
                };
                let str = if let Some(val) = get_string(ptr) {
                    // I am not 100% sure we are properly disposing of the original cstring
                    // Does to_string clone the data?
                    // Does Arc clone the data?
                    // we just call clone here so we can "safely" drop the Cstring
                    let re = val.clone();

                    unsafe {
                        plugin_handle.free_string_ptr(ptr);
                    }
                    re
                } else {
                    return false;
                };

                // TODO deal with async locking
                let _res = mu.lock();
                // *res = str;

                false
            },
            (PropertyType::Duration, ValueContainer::Dur(at)) => {
                let d = unsafe { val.value.dur };
                at.store(d, SAVE_ORDERING);

                true
            },
            (PropertyType::Str, _) => {
                // Deallocating the string, even if this is a missmatch
                unsafe {
                    let ptr = val.value.str;
                    plugin_handle.free_string_ptr(ptr);
                }
                false
            },
            _ => false
        }
    }

    async fn update_int(&self, val: Value, prop_handle: &PropertyHandle) -> bool {
        match (val,self) {
            (Value::None, ValueContainer::None) => true,
            (Value::Int(i), ValueContainer::Int(at)) => {
                at.store(i, SAVE_ORDERING);
                true
            },
            (Value::Float(f), ValueContainer::Float(at)) => {
                at.store(f, SAVE_ORDERING);
                true
            },
            (Value::Bool(b), ValueContainer::Bool(at)) => {
                at.store(b, SAVE_ORDERING);
                true
            },
            (Value::Str(s),ValueContainer::Str(mu)) => {
                let mut res = mu.lock().await;
                *res = s.clone();

                // for (sub,_) in listener.iter() {
                //     let _ = sub.send(Message::Update(prop_handle.clone(),Value::Str(s.clone()))).await;
                // }

                true
            },
            (Value::Dur(d), ValueContainer::Dur(at)) => {
                at.store(d, SAVE_ORDERING);
                true
            },
            _ => false,
        }
    }

    pub(crate) fn read(&self) -> Property {
        match self {
            ValueContainer::None => Property::default(),
            ValueContainer::Int(at) => Property {
                sort: PropertyType::Int,
                value: PropertyValue { integer: at.load(READ_ORDERING) }
            },
            ValueContainer::Float(at) => Property {
                sort: PropertyType::Float,
                value: {
                    let conv = at.load(READ_ORDERING);
                    PropertyValue { decimal: f64::from_be_bytes(conv.to_be_bytes()) }
                }
            },
            ValueContainer::Bool(at) => Property {
                sort: PropertyType::Boolean,
                value: PropertyValue { boolean: at.load(READ_ORDERING) }
            },
            ValueContainer::Str(mu) => Property::default(),
            ValueContainer::Dur(at) => {
                Property {
                    sort: PropertyType::Duration,
                    value: PropertyValue { dur: at.load(READ_ORDERING) }
                }
            }
        }
    }

    async fn read_int(&self) -> Value {
        match self {
            ValueContainer::None => Value::None,
            ValueContainer::Int(at) => Value::Int(at.load(READ_ORDERING)),
            ValueContainer::Float(at) => Value::Float(at.load(READ_ORDERING)),
            ValueContainer::Bool(at) => Value::Bool(at.load(READ_ORDERING)),
            ValueContainer::Str(mu) => { 
                let res = mu.lock().await;
                let a = res.clone();
                Value::Str(a)
            },
            ValueContainer::Dur(at) => Value::Dur(at.load(READ_ORDERING)),
        }
    }
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

const HASH_KEY_NAME:Key = Key([1,2,3,4]);

/// Serves to generate hashes for the name of a plugin
pub(crate) fn generate_plugin_name_hash(str: &str) -> Option<u64> {
    if str.contains('.') {
        return None;
    }
    let str = str.to_lowercase();

    let mut hasher = HighwayHasher::new(HASH_KEY_NAME);

    hasher.append(str.as_bytes());

    Some(hasher.finalize64())
}


const HASH_KEY_PROPERTY:Key = Key([2,4,3,4]);

/// Serves to generate hashes for the name of a plugin
pub(crate) fn generate_property_name_hash(str: &str) -> Option<u64> {
    if str.strip_suffix('.').is_some() || str.strip_prefix('.').is_some() {
        return None;
    }
    let str = str.to_lowercase();

    let mut hasher = HighwayHasher::new(HASH_KEY_PROPERTY);

    hasher.append(str.as_bytes());

    Some(hasher.finalize64())
}
