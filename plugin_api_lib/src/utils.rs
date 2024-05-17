use libc::c_char;
use serde::{Deserialize, Serialize}; 
use std::{ffi::{CStr, CString}, sync::{atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering}, Arc, RwLock}};
use kanal::{Sender, Receiver};
use highway::{HighwayHash, HighwayHasher, Key};

use crate::{pluginloader::LoaderMessage, PluginHandle, Property, PropertyType, PropertyValue};

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

    pub(crate) fn swap_container(&mut self, container: ValueContainer, allow_modify: bool) {
        self.value = container;
        self.allow_modify = allow_modify;
    }

    /// This generates a linked clone of the ValueContainer, which shares any value updates
    pub(crate) fn clone_container(&self) -> ValueContainer {
        self.value.shallow_clone()
    }
}

#[derive(Debug)]
pub(crate) enum ValueContainer {
    None,
    Int(Arc<AtomicI64>),
    Float(Arc<AtomicU64>),
    Bool(Arc<AtomicBool>),
    Str(Arc<RwLock<String>>),
    Dur(Arc<AtomicI64>)
}

const SAVE_ORDERING: Ordering = Ordering::Release;
const READ_ORDERING: Ordering = Ordering::Acquire;

impl ValueContainer {
    pub(crate) fn new(val: Property, plugin_handle: &PluginHandle) -> Self {
        let new = match val.sort {
            PropertyType::None => ValueContainer::None,
            PropertyType::Int => ValueContainer::Int(Arc::default()),
            PropertyType::Float => ValueContainer::Float(Arc::default()),
            PropertyType::Boolean => ValueContainer::Bool(Arc::default()),
            PropertyType::Str => ValueContainer::Str(Arc::default()),
            PropertyType::Duration => ValueContainer::Dur(Arc::default())
        };
        new.update(val, plugin_handle);

        new
    }


    // fn new_int(val: Value) -> ValueContainer {
    //     match val {
    //         Value::None => ValueContainer::None,
    //         Value::Int(i) => ValueContainer::Int(AtomicI64::new(i)),
    //         Value::Float(f) => ValueContainer::Float(AtomicU64::new(f)),
    //         Value::Bool(b) => ValueContainer::Bool(AtomicBool::new(b)),
    //         Value::Str(s) => ValueContainer::Str(Mutex::new(s)),
    //         Value::Dur(d) => ValueContainer::Dur(AtomicI64::new(d))
    //     }
    //     ValueContainer::None
    // }

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
            (PropertyType::Str, ValueContainer::Str(store)) => {
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

                let mut res = match store.write() {
                    Ok(res) => res,
                    Err(e) => {
                        store.clear_poison();
                        e.into_inner()
                    }
                };
                *res = str;
                drop(res);

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

    // async fn update_int(&self, val: Value, prop_handle: &PropertyHandle) -> bool {
    //     match (val,self) {
    //         (Value::None, ValueContainer::None) => true,
    //         (Value::Int(i), ValueContainer::Int(at)) => {
    //             at.store(i, SAVE_ORDERING);
    //             true
    //         },
    //         (Value::Float(f), ValueContainer::Float(at)) => {
    //             at.store(f, SAVE_ORDERING);
    //             true
    //         },
    //         (Value::Bool(b), ValueContainer::Bool(at)) => {
    //             at.store(b, SAVE_ORDERING);
    //             true
    //         },
    //         (Value::Str(s),ValueContainer::Str(mu)) => {
    //             let mut res = mu.lock().await;
    //             *res = s.clone();
    //
    //             // for (sub,_) in listener.iter() {
    //             //     let _ = sub.send(Message::Update(prop_handle.clone(),Value::Str(s.clone()))).await;
    //             // }
    //
    //             true
    //         },
    //         (Value::Dur(d), ValueContainer::Dur(at)) => {
    //             at.store(d, SAVE_ORDERING);
    //             true
    //         },
    //         _ => false,
    //     }
    // }

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
            ValueContainer::Str(store) => {
                let res = match store.read() {
                    Ok(res) => {
                        res.clone()
                    },
                    Err(_) => {
                        // As we don't have write access, and there seems to be poison, we return a
                        // blank string as fallback, as technically there is no garantee the string
                        // object stored is in one piece.
                        // But in reality this case should be pratically impossible, we only write
                        // a single string while holding this handle, this should never go wrong
                        "".to_string()
                    }
                };
                
                
                let raw = CString::new(res).expect("string is string").into_raw();

                Property {
                    sort: PropertyType::Str,
                    value: PropertyValue { str: raw }
                }
                
            },
            ValueContainer::Dur(at) => {
                Property {
                    sort: PropertyType::Duration,
                    value: PropertyValue { dur: at.load(READ_ORDERING) }
                }
            }
        }
    }

    pub(crate) fn read_web(&self) -> Value {
        match self {
            ValueContainer::None => Value::None,
            ValueContainer::Int(at) => Value::Int(at.load(READ_ORDERING)),
            ValueContainer::Float(at) => Value::Float(at.load(READ_ORDERING)),
            ValueContainer::Bool(at) => Value::Bool(at.load(READ_ORDERING)),
            ValueContainer::Str(store) => { 
                let res = match store.read() {
                    Ok(res) => {
                        res.clone()
                    },
                    Err(_) => {
                        // As we don't have write access, and there seems to be poison, we return a
                        // blank string as fallback, as technically there is no garantee the string
                        // object stored is in one piece.
                        // But in reality this case should be pratically impossible, we only write
                        // a single string while holding this handle, this should never go wrong
                        "".to_string()
                    }
                };
                Value::Str(res)
            },
            ValueContainer::Dur(at) => Value::Dur(at.load(READ_ORDERING)),
        }
    }

    /// This generates a shallow clone, which still receives all the same value changes
    fn shallow_clone(&self) -> ValueContainer {
        match self {
            ValueContainer::None => ValueContainer::None,
            ValueContainer::Int(a) => {
                let b = a.clone();
                ValueContainer::Int(b)
            },
            ValueContainer::Float(a) => ValueContainer::Float(a.clone()),
            ValueContainer::Bool(a) => ValueContainer::Bool(a.clone()),
            ValueContainer::Str(a) => ValueContainer::Str(a.clone()),
            ValueContainer::Dur(a) => ValueContainer::Dur(a.clone())
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) enum Value {
    None,
    Int(i64),
    Float(u64),
    Bool(bool),
    Str(String),
    Dur(i64)
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
