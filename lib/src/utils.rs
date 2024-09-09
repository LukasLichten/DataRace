use libc::c_char;
use serde::{Deserialize, Serialize}; 
use std::{ffi::{CStr, CString}, fmt::Debug, sync::{atomic::{AtomicBool, AtomicI64, AtomicU64, AtomicUsize, Ordering}, Arc, RwLock}};
use kanal::{Sender, Receiver};
use highway::{HighwayHash, HighwayHasher, Key};

use crate::{pluginloader::LoaderMessage, DataStoreReturnCode, PluginHandle, Property, PropertyType, PropertyValue};

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

/// For handling void pointers send from plugins to other plugins
#[derive(Debug)]
pub(crate) struct VoidPtrWrapper {
    pub ptr: *mut libc::c_void
}

unsafe impl Send for VoidPtrWrapper {}
unsafe impl Sync for VoidPtrWrapper {}

#[derive(Debug, PartialEq, Clone)]
pub(crate) enum PluginStatus {
    Init,
    Running,
    // ShutingDown,
    // Crashed
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
            match val.sort {
                PropertyType::Str => {
                    unsafe {
                        plugin_handle.free_string_ptr(val.value.str);
                    }
                },
                PropertyType::Array => {
                    unsafe {
                        if !val.value.arr.is_null() {
                            val.value.arr.drop_in_place()
                        }
                    }
                },
                _ => ()
            }

            return false;
        }

        self.value.update(val, plugin_handle)
    }

    pub(crate) fn read(&self) -> Property {
        self.value.read(self.allow_modify)
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
    Str(Arc<(RwLock<String>,AtomicUsize)>),
    Dur(Arc<AtomicI64>),
    Arr(Arc<ArrayValueContainer>)
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
            PropertyType::Duration => ValueContainer::Dur(Arc::default()),

            PropertyType::Array => {
                let ptr = unsafe { 
                    val.value.arr
                };

                if !ptr.is_null() {
                    let handle = unsafe {
                        ptr.read()
                    };

                    
                    // We have to return here, as otherwise our value will get ingested into
                    // update call too, despite us already having taken ownership.
                    // The result is a double deallocation of the ArrayValueHandle, decreasing the
                    // Arc to 0 references (even though we have one here) and dropping it too
                    return if handle.allow_modify {
                        // We only allow storage of those that we have modify for
                        ValueContainer::Arr(handle.arr.clone())
                    } else {
                        ValueContainer::None
                    };
                } else {
                    ValueContainer::None
                }
            }
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
            (PropertyType::Str, ValueContainer::Str(arc)) => {
                let ptr = unsafe {
                    val.value.str
                };

                write_string(ptr, &arc.0, &arc.1, plugin_handle)
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
            (PropertyType::Array, _) => {
                // Deallocating Array Handle, as passing in this type on update is not permitted,
                // but still not allowed
                unsafe {
                    if !val.value.arr.is_null() {
                        val.value.arr.drop_in_place();
                    }
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

    pub(crate) fn read(&self, allow_modify: bool) -> Property {
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
            ValueContainer::Str(arc) => {
                let store = &arc.0;
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
            },
            ValueContainer::Arr(arr) => {
                let arr = arr.clone();
                let arr_handle = crate::ArrayValueHandle { arr, allow_modify };
                
                Property {
                    sort: PropertyType::Array,
                    value: PropertyValue { arr: Box::into_raw(Box::new(arr_handle)) }
                }
            }
        }
    }

    pub(crate) fn read_web(&self, cache: &mut ValueCache) -> bool {
        let val = match self {
            ValueContainer::None => Value::None,
            ValueContainer::Int(at) => Value::Int(at.load(READ_ORDERING)),
            ValueContainer::Float(at) => Value::Float(f64::from_be_bytes(at.load(READ_ORDERING).to_be_bytes())),
            ValueContainer::Bool(at) => Value::Bool(at.load(READ_ORDERING)),
            ValueContainer::Str(arc) => {
                let (store, index) = (&arc.0, arc.1.load(Ordering::Acquire));
                
                if let Some(old_index) = cache.version {
                    if old_index == index {
                        return false;
                    }
                }

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
                
                cache.version = Some(index);
                cache.value = Value::Str(res);
                return true;
            },
            ValueContainer::Dur(at) => Value::Dur(at.load(READ_ORDERING)),
            ValueContainer::Arr(_) => todo!("Implement web reading of Array")
        };

        if cache.value == val {
            false
        } else {
            cache.value = val;
            true
        }
    }

    /// This generates a shallow clone, which still receives all the same value changes
    fn shallow_clone(&self) -> ValueContainer {
        match self {
            ValueContainer::None => ValueContainer::None,
            ValueContainer::Int(a) => ValueContainer::Int(a.clone()),
            ValueContainer::Float(a) => ValueContainer::Float(a.clone()),
            ValueContainer::Bool(a) => ValueContainer::Bool(a.clone()),
            ValueContainer::Str(a) => ValueContainer::Str(a.clone()),
            ValueContainer::Dur(a) => ValueContainer::Dur(a.clone()),
            ValueContainer::Arr(arr) => ValueContainer::Arr(arr.clone())
        }
    }
}

fn write_string(ptr: *mut c_char, store: &RwLock<String>, version: &AtomicUsize, plugin_handle: &PluginHandle) -> bool {
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
    version.fetch_add(1, Ordering::AcqRel);
    drop(res);

    true
}

#[derive(Debug)]
pub(crate) enum ArrayValueContainer {
    Int(Box<[AtomicI64]>),
    Float(Box<[AtomicU64]>),
    Bool(Box<[AtomicBool]>),
    Str(Box<[(RwLock<String>, AtomicUsize)]>),
    Dur(Box<[AtomicI64]>),

    // Arr(Arc<[ArrayValueContainer]>)
    // Multilayer arrays present multiple issues:
    // - Type definition (insuring every ArrayValueContainer has the same type)
    // - Size definition (should nested be allowed different sizes?)
    // - Should we allow replacing One nested array with another
    // And how these settings are set during creation...
    //
    // As n fixed sized nested arrays (with m length) could just be converted into a one array with
    // n * m length, and that is more then accessible enough for now, but we could explore this for
    // future use
}

macro_rules! array_read {
    ($arc:ident, $index:ident) => {
        if let Some(item) = $arc.get($index) {
            item.load(READ_ORDERING)
        } else {
            return Property::default();
        }
    };
}

macro_rules! array_write {
    ($arc:ident, $index:ident, $value:ident) => {
        if let Some(item) = $arc.get($index) {
            item.store($value, SAVE_ORDERING);
            DataStoreReturnCode::Ok
        } else {
            DataStoreReturnCode::DoesNotExist
        }
    };
}

macro_rules! array_create {
    ($def:ident, $size:ident, $type:ident) => {
        {
            let mut v = Vec::<$type>::with_capacity($size);
            
            for _ in 0..$size {
                v.push($type::new($def));
            }

            v.into_boxed_slice()
        }
    };
}

impl ArrayValueContainer {
    pub(crate) fn new(size: usize, init: Property, plugin_handle: &PluginHandle) -> Option<Self> {
        Some(match init.sort {
            PropertyType::Int => {
                let val = unsafe {
                    init.value.integer
                };
                
                ArrayValueContainer::Int(array_create!(val, size, AtomicI64))
            },
            PropertyType::Float => {
                let val = u64::from_be_bytes(unsafe {
                    init.value.decimal
                }.to_be_bytes());

                ArrayValueContainer::Float(array_create!(val, size, AtomicU64))
            },
            PropertyType::Boolean => {
                let val = unsafe {
                    init.value.boolean
                };

                ArrayValueContainer::Bool(array_create!(val, size, AtomicBool))
            },
            PropertyType::Str => {
                let ptr = unsafe {
                    init.value.str
                };

                let mut v = Vec::<(RwLock<String>, AtomicUsize)>::with_capacity(size);

                if let Some(t) = get_string(ptr) {
                    for _ in 0..size {
                        v.push((RwLock::new(t.clone()), AtomicUsize::new(1)));
                        // why 1? because a regular valuecontainer would also have 1 after init.
                        // Because there we init default values, and run update to get the true init values,
                        // while here we init with the passed in values.
                        // Honestly, should not make any difference
                    }

                    drop(t);
                    unsafe {
                        plugin_handle.free_string_ptr(ptr);
                    }
                } else {
                    return None;
                } 

                ArrayValueContainer::Str(v.into_boxed_slice())
            },
            PropertyType::Duration => {
                let val = unsafe {
                    init.value.dur
                };

                ArrayValueContainer::Dur(array_create!(val, size, AtomicI64))
            },
            _ => None?
        })
    }

    pub(crate) fn read(&self, index: usize) -> Property {
        match self {
            Self::Int(arc) => {
                Property { sort: PropertyType::Int, value: PropertyValue { integer: array_read!(arc, index) } }
            },
            Self::Float(arc) => {
                Property { sort: PropertyType::Float, value: PropertyValue { decimal: f64::from_be_bytes(array_read!(arc, index).to_be_bytes())  } }
            },
            Self::Bool(arc) => {
                Property { sort: PropertyType::Boolean, value: PropertyValue { boolean: array_read!(arc, index) } }
            },
            Self::Str(arc) => {
                if let Some(item) = arc.get(index) {
                    let store = &item.0;
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

                } else {
                    Property::default()
                }
            },
            Self::Dur(arc) => {
                Property { sort: PropertyType::Duration, value: PropertyValue { dur: array_read!(arc, index) } }
            },
            // Self::Arr(arc) => {
            //     if let Some(item) = arc.get(index) {
            //         let arr = item.clone();
            //         let arr_handle = crate::ArrayValueHandle { arr };
            //
            //         Property {
            //             sort: PropertyType::Array,
            //             value: PropertyValue { arr: Box::into_raw(Box::new(arr_handle)) }
            //         }
            //     } else {
            //         Property::default()
            //     }
            // }
        }
    }

    pub(crate) fn write(&self, index: usize, value: Property, plugin_handle: &PluginHandle) -> DataStoreReturnCode {
        match (self,value.sort) {
            (Self::Int(arc),PropertyType::Int) => {
                let val = unsafe { value.value.integer };
                array_write!(arc, index, val)
            },
            (Self::Float(arc),PropertyType::Float) => {
                let val = u64::from_be_bytes(unsafe { value.value.decimal }.to_be_bytes());
                array_write!(arc, index, val)
            },
            (Self::Bool(arc),PropertyType::Boolean) => {
                let val = unsafe { value.value.boolean };
                array_write!(arc, index, val)
            },
            (Self::Str(arc),PropertyType::Str) => {
                if let Some((store, version)) = arc.get(index) {
                    let ptr = unsafe {
                        value.value.str
                    };

                    if write_string(ptr, store, version, plugin_handle) {
                        DataStoreReturnCode::Ok
                    } else {
                        DataStoreReturnCode::ParameterCorrupted
                    }
                } else {
                    DataStoreReturnCode::DoesNotExist
                }
            },
            (Self::Dur(arc),PropertyType::Duration) => {
                let val = unsafe { value.value.dur };
                array_write!(arc, index, val)
            },
            (_, PropertyType::Str) => {
                // Deallocating the string, even on a type missmatch
                unsafe {
                    let ptr = value.value.str;
                    plugin_handle.free_string_ptr(ptr);
                }

                DataStoreReturnCode::TypeMissmatch
            },
            (_, PropertyType::Array) => {
                // Deallocating arrayhandle, as we were given ownership of it
                unsafe {
                    if !value.value.arr.is_null() {
                        value.value.arr.drop_in_place();
                    }
                }

                DataStoreReturnCode::TypeMissmatch
            },
            (_, _) => DataStoreReturnCode::TypeMissmatch
        }
    }

    pub(crate) fn length(&self) -> usize {
        match self {
            Self::Int(arr) => arr.len(),
            Self::Float(arr) => arr.len(),
            Self::Bool(arr) => arr.len(),
            Self::Str(arr) => arr.len(),
            Self::Dur(arr) => arr.len(),

        }
    }

    pub(crate) fn get_type(&self) -> PropertyType {
        match self {
            Self::Int(_) => PropertyType::Int,
            Self::Float(_) => PropertyType::Float,
            Self::Bool(_) => PropertyType::Boolean,
            Self::Str(_) => PropertyType::Str,
            Self::Dur(_) => PropertyType::Duration,
        }
    }
}

/// Serves to define the datatype for nested Array types
// #[derive(Debug, Clone)]
// pub(crate) enum ArrayContainerType {
//     Int,
//     Float,
//     Bool,
//     Str,
//     Dur,
//     Arr(Arc<ArrayContainerType>)
// }

/// Cache for polling values internally
#[derive(Debug, Clone)]
pub(crate) struct ValueCache {
    pub value: Value,
    version: Option<usize>
}

impl Default for ValueCache {
    fn default() -> Self {
        ValueCache { value: Value::None, version: None }
    }
}

/// A single Value, but for internal and web use
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub(crate) enum Value {
    None,
    Int(i64),
    Float(f64),
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
