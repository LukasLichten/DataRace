use libc::c_char; 
use std::{ffi::{CStr, CString}, sync::{Arc, atomic::{AtomicU64, AtomicI64, AtomicBool, Ordering}}};
use kanal::{Sender, AsyncSender, Receiver};

use tokio::sync::Mutex;

use crate::{pluginloader::Message, Property, PropertyType, PropertyHandle};

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

pub fn get_message_channel() -> (Sender<Message>, Receiver<Message>) {
    kanal::unbounded()
}



#[derive(Debug)]
pub(crate) enum ValueContainer {
    None,
    Int(AtomicI64),
    Float(AtomicU64),
    Bool(AtomicBool),
    Str(Mutex<Arc<String>>, Vec<(AsyncSender<Message>, u64)>),
    Dur(AtomicI64)
}

impl ValueContainer {
    fn new(val: Value) -> ValueContainer {
        match val {
            Value::None => ValueContainer::None,
            Value::Int(i) => ValueContainer::Int(AtomicI64::new(i)),
            Value::Float(f) => ValueContainer::Float(AtomicU64::new(f)),
            Value::Bool(b) => ValueContainer::Bool(AtomicBool::new(b)),
            Value::Str(s) => ValueContainer::Str(Mutex::new(s), Vec::default()),
            Value::Dur(d) => ValueContainer::Dur(AtomicI64::new(d))
        }
    }

    async fn update(&self, val: Value, prop_handle: &PropertyHandle) -> bool {
        match (val,self) {
            (Value::None, ValueContainer::None) => true,
            (Value::Int(i), ValueContainer::Int(at)) => {
                at.store(i, Ordering::Release);
                true
            },
            (Value::Float(f), ValueContainer::Float(at)) => {
                at.store(f, Ordering::Release);
                true
            },
            (Value::Bool(b), ValueContainer::Bool(at)) => {
                at.store(b, Ordering::Release);
                true
            },
            (Value::Str(s),ValueContainer::Str(mu, listener)) => {
                let mut res = mu.lock().await;
                *res = s.clone();

                for (sub,_) in listener.iter() {
                    let _ = sub.send(Message::Update(prop_handle.clone(),Value::Str(s.clone()))).await;
                }

                true
            },
            (Value::Dur(d), ValueContainer::Dur(at)) => {
                at.store(d, Ordering::Release);
                true
            },
            _ => false,
        }
    }

    async fn read(&self) -> Value {
        match self {
            ValueContainer::None => Value::None,
            ValueContainer::Int(at) => Value::Int(at.load(Ordering::Acquire)),
            ValueContainer::Float(at) => Value::Float(at.load(Ordering::Acquire)),
            ValueContainer::Bool(at) => Value::Bool(at.load(Ordering::Acquire)),
            ValueContainer::Str(mu,_) => { 
                let res = mu.lock().await;
                let a = res.clone();
                Value::Str(a)
            },
            ValueContainer::Dur(at) => Value::Dur(at.load(Ordering::Acquire)),
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

