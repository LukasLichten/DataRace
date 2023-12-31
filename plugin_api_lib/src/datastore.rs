use std::sync::{atomic::{AtomicU64, AtomicI64, AtomicBool, Ordering}, Arc};
use std::hash::{Hasher,Hash};

use fnv::{FnvHashMap,FnvHasher};
use log::{debug, error};
use tokio::sync::{RwLock, Mutex};
use kanal::{Sender, Receiver};
use rand::{RngCore, SeedableRng};
use rand_hc::Hc128Rng;

use crate::{pluginloader::Message, DataStoreReturnCode, PropertyHandle, utils::Value};

/// This is our centralized State
pub(crate) struct DataStore {
    // Definitly some optimizations can be made here
    property_map: FnvHashMap<String, usize>,
    propertys: Vec<Property>,
    plugins: Vec<Plugin>
}

impl DataStore {
    pub fn new() -> RwLock<DataStore> {
        RwLock::new(DataStore {
            property_map: FnvHashMap::default(), 
            propertys: Vec::<Property>::new(),
            plugins: Vec::<Plugin>::new()
        })
    }

    pub(crate) async fn create_property(&mut self, token: &Token, name: String, value: Value) -> Result<PropertyHandle,DataStoreReturnCode> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DataStoreReturnCode::AlreadyExists);
        }

        if let Some(plugin_name) = self.get_plugin_name_from_token(token) {
            let property_name = format!("{}.{}", plugin_name, name);

            // Property seems to exist already...
            // Placeholders could have been created by listeners
            if let Some(address) = self.property_map.get(&property_name) {
                let address = address.clone();
                if let Some(item) = self.propertys.get_mut(address) {
                    if item.plugin_token == Token::default() && property_name == item.name {
                        // This is a placeholder, we replace the token, set Value, and ping
                        // listeners
                        
                        item.plugin_token = token.clone();
                        item.value = ValueContainer::new(value.clone());

                        // We can't drop w_list, it would mean cloning all listeners
                        // Even though this means if the listener immediatly acts and tries to
                        // access propertys will find it locked
                        // However, as this should only happen once, in regular updates we only
                        // need read lock

                        for p in item.listeners.iter() {
                            // Out creation could be stalled by a full message queue...
                            let _ = p.as_async().send(Message {}).await;
                        }

                        return Ok(PropertyHandle { index: address, hash: item.name_hash.clone() });
                    } else if property_name == item.name {
                        // This is not a placeholder we can not claim it
                        return Err(DataStoreReturnCode::AlreadyExists);
                    } else {
                        // For some reason the hashmap takes us here, but this is not the address
                        error!("Corruption in the datastore: map pointes to item of a different name");
                        return Err(DataStoreReturnCode::DataCorrupted);
                    }
                } else {
                    // Something is corrupted here
                    // TODO: We could fix it, removing the bad reference from the HashMap, but what
                    // if the item exists somewhere else? We would need to iterate through all
                    // properties to check
                    error!("Corruption in the datastore: map contains pointer beyond array");
                    return Err(DataStoreReturnCode::DataCorrupted);
                }
            }

            // Normal case: We create a new Property, and insert it
            let mut hasher = FnvHasher::default();
            property_name.hash(&mut hasher);

            let name_hash = hasher.finish();

            self.propertys.push(Property { 
                name: property_name.clone(), 
                name_hash, 
                plugin_token: token.clone(), 
                listeners: vec![],
                value: ValueContainer::new(value)
            });

            let index = self.propertys.len() - 1;

            self.property_map.insert(property_name, index);

            return Ok(PropertyHandle { index, hash: name_hash });
        } else {
            // Not Autheticated
            return Err(DataStoreReturnCode::NotAuthenticated);
        }
    }

    pub(crate) async fn update_property(&self, token: &Token, handle: &PropertyHandle, value: Value) -> DataStoreReturnCode {
        if let Some(item) = self.propertys.get(handle.index.clone()) {
            if item.name_hash != handle.hash {
                return DataStoreReturnCode::OutdatedPropertyHandle;
            }

            if &item.plugin_token != token {
                return DataStoreReturnCode::NotAuthenticated;
            }

            if let Some(_value) = item.value.update(value).await {
                for p in item.listeners.iter() {
                    let _ = p.as_async().send(Message {} ).await;
                }

                DataStoreReturnCode::Ok
            } else {
                DataStoreReturnCode::TypeMissmatch
            }
        } else {
            DataStoreReturnCode::OutdatedPropertyHandle
        }
    }

#[allow(unused_variables, dead_code)]
    pub(crate) async fn change_property_name(&self, token: &Token, handle: &PropertyHandle, name: String) -> Result<PropertyHandle, DataStoreReturnCode> {
        todo!()
    }

#[allow(unused_variables, dead_code)]
    pub(crate) async fn change_property_type(&self, token: &Token, handle: &PropertyHandle, value: Value) -> DataStoreReturnCode {
        todo!();
    }

    pub(crate) async fn subscribe_to_property(&mut self, token: &Token, handle: &PropertyHandle) -> DataStoreReturnCode {
        if let Some(item) = self.propertys.get_mut(handle.index.clone()) {
            if item.name_hash != handle.hash {
                return DataStoreReturnCode::OutdatedPropertyHandle;
            }
            
            if item.value.is_atomic() {
                // We are sending a message into the pluginmanager to poll this property, as this
                // is more performant
                for p in self.plugins.iter() {
                    if &p.token == token {
                        if p.channel.as_async().send(Message { }).await.is_ok() {
                            return DataStoreReturnCode::Ok
                        } else {
                            // We don't have a special case for when this happens
                            // Should happen only when the channel is closed, so after shutdown but
                            // some thread might not be shut down yet
                            return DataStoreReturnCode::NotAuthenticated;
                        }
                    }
                }

                DataStoreReturnCode::NotAuthenticated
            } else {
                for p in self.plugins.iter() {
                    if &p.token == token {
                        item.listeners.push(p.channel.clone());
                        return DataStoreReturnCode::Ok
                    }
                }

                DataStoreReturnCode::NotAuthenticated
            }
        } else {
            DataStoreReturnCode::OutdatedPropertyHandle
        }
    }

    pub(crate) async fn unsubscribe_from_property(&mut self, token: &Token, handle: &PropertyHandle) -> DataStoreReturnCode {
        if let Some(item) = self.propertys.get_mut(handle.index.clone()) {
            if item.name_hash != handle.hash {
                return DataStoreReturnCode::OutdatedPropertyHandle;
            }

            for p in self.plugins.iter() {
                if &p.token == token {
                    let _ = p.channel.as_async().send(Message {}).await;
                    
                    // item.listeners.retain(|l| l != &p.channel);
                    return DataStoreReturnCode::Ok;
                }
            }

            return DataStoreReturnCode::NotAuthenticated;
        }

        DataStoreReturnCode::OutdatedPropertyHandle
    }

    pub(crate) async fn delete_property(&mut self, token: &Token, handle: &PropertyHandle) -> DataStoreReturnCode {
        if let Some(item) = self.propertys.get_mut(handle.index.clone()) {
            if item.name_hash != handle.hash {
                return DataStoreReturnCode::OutdatedPropertyHandle;
            }

            if &item.plugin_token != token {
                return DataStoreReturnCode::NotAuthenticated;
            }

            // Everything is authenticated
            self.property_map.remove(&item.name);

            // We can't delete items out of the vector, we have to empty them
            item.name = String::new();
            item.name_hash = 0;
            item.plugin_token = Token::default();
            item.value = ValueContainer::None;

            for p in item.listeners.iter() {
                let _ = p.as_async().send(Message {} ).await;
            }
            item.listeners.clear();

            // TODO: keep track of empty cells to repopulate

            DataStoreReturnCode::Ok
        } else {
            DataStoreReturnCode::OutdatedPropertyHandle
        }
    }

    pub(crate) fn get_property_handle(&self, name: String) -> Result<PropertyHandle, DataStoreReturnCode> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(DataStoreReturnCode::DoesNotExist);
        }

        if let Some(address) = self.property_map.get(&name) {
            let address = address.clone();

            if let Some(item) = self.propertys.get(address) {
                if item.name != name {
                    panic!("Corrupted Datastore: Map links to property that is not the property");
                }

                Ok(PropertyHandle { index: address, hash: item.name_hash })
            } else {
                panic!("Corrupted Datastore: Map links into an index outside of the list");
            }
        } else {
            Err(DataStoreReturnCode::DoesNotExist)
        }
    }

    pub(crate) async fn get_property(&self, handle: &PropertyHandle) -> Result<Value, DataStoreReturnCode> {
        if let Some(item) = self.propertys.get(handle.index.clone()) {
            if item.name_hash != handle.hash {
                return Err(DataStoreReturnCode::OutdatedPropertyHandle);
            }
            
            Ok(item.value.read().await)
        } else {
            Err(DataStoreReturnCode::OutdatedPropertyHandle)
        }
    }

    pub(crate) fn create_plugin(&mut self, name: String) -> Option<(Token, Receiver<Message>, Sender<Message>)> {
        if name.trim().is_empty() {
            return None;
        }

        
        let mut token = Token::default();
        while token == Token::default() {
            token = Token::new();

            for plugin in self.plugins.iter() {
                if plugin.token == token {
                    token = Token::default();
                    break;
                } else if name == plugin.name {
                    return None;
                }
            }
        }

        let (sx,rx) = kanal::unbounded();
        
        self.plugins.push(Plugin { name, token: token.clone(), channel: sx.clone() });

        Some((token, rx, sx))
    }

    pub(crate) async fn delete_plugin(&mut self, token: &Token) -> DataStoreReturnCode {
        for p in self.plugins.iter_mut() {
            if &p.token == token {
                // We can't remove a plugin as it would change the index of items
                p.name = String::new();
                p.token = Token::default();
                
                //Can't unset this, but the channel should be closed automatically anyway
                //p.channel = None

                // Need to delete all properties tied to this plugins
                for index in 0..self.propertys.len() {
                    if let Some(item) = self.propertys.get_mut(index) {
                        if &item.plugin_token == token {
                            debug!("Cleaning up property {} with value {}", &item.name, if let ValueContainer::Int(i) = &item.value { i.load(Ordering::Acquire).to_string() } else { "Error".to_string() });
                            // We have to delete this plugin
                            self.property_map.remove(&item.name);

                            item.name = String::new();
                            item.name_hash = 0;
                            item.plugin_token = Token::default();
                            item.value = ValueContainer::None;

                            for p in item.listeners.iter() {
                                let _ = p.as_async().send(Message {} ).await;
                            }
                            item.listeners.clear();
                        }
                    }
                }
                
                // We should also remove the subscription of every property, but that is way way to
                // expensive, and the delete_plugin should really only be called during crashes and shutdown
                // At least during shutdown this would be a massive waste of time
                
                return DataStoreReturnCode::Ok;
            }
        }

        DataStoreReturnCode::DoesNotExist
    } 
    
    fn get_plugin_name_from_token(&self, token: &Token) -> Option<String> {
        if token == &Token::default() {
            // default is reserved for none/owned by Datastore
            return None;
        }

        // This is highly inefficient, but considering there is only like 2 dozend plugins this
        // should never be a problem
        // And we only need this when registering Plugins
        for p in self.plugins.iter() {
            if &p.token == token {
                return Some(p.name.clone());
            }
        }

        None
    }
}

pub(crate) struct Property {
    name: String,
    name_hash: u64,
    plugin_token: Token,
    listeners: Vec<Sender<Message>>,
    value: ValueContainer
}

unsafe impl Send for Property {}

#[derive(Debug)]
pub(crate) enum ValueContainer {
    None,
    Int(AtomicI64),
    Float(AtomicU64),
    Bool(AtomicBool),
    Str(Mutex<Arc<String>>),
    Dur(AtomicI64)
}

impl ValueContainer {
    fn new(val: Value) -> ValueContainer {
        match val {
            Value::None => ValueContainer::None,
            Value::Int(i) => ValueContainer::Int(AtomicI64::new(i)),
            Value::Float(f) => ValueContainer::Float(AtomicU64::new(f)),
            Value::Bool(b) => ValueContainer::Bool(AtomicBool::new(b)),
            Value::Str(s) => ValueContainer::Str(Mutex::new(s)),
            Value::Dur(d) => ValueContainer::Dur(AtomicI64::new(d))
        }
    }

    async fn update(&self, val: Value) -> Option<Value> {
        Some(match (val,self) {
            (Value::None, ValueContainer::None) => Value::None,
            (Value::Int(i), ValueContainer::Int(at)) => {
                at.store(i, Ordering::Release);
                Value::Int(i)
            },
            (Value::Float(f), ValueContainer::Float(at)) => {
                at.store(f, Ordering::Release);
                Value::Float(f)
            },
            (Value::Bool(b), ValueContainer::Bool(at)) => {
                at.store(b, Ordering::Release);
                Value::Bool(b)
            },
            (Value::Str(s),ValueContainer::Str(mu)) => {
                let mut res = mu.lock().await;
                *res = s.clone();
                Value::Str(s)
            },
            (Value::Dur(d), ValueContainer::Dur(at)) => {
                at.store(d, Ordering::Release);
                Value::Dur(d)
            },
            _ => return None,
        })
    }

    async fn read(&self) -> Value {
        match self {
            ValueContainer::None => Value::None,
            ValueContainer::Int(at) => Value::Int(at.load(Ordering::Acquire)),
            ValueContainer::Float(at) => Value::Float(at.load(Ordering::Acquire)),
            ValueContainer::Bool(at) => Value::Bool(at.load(Ordering::Acquire)),
            ValueContainer::Str(mu) => { 
                let res = mu.lock().await;
                let a = res.clone();
                Value::Str(a)
            },
            ValueContainer::Dur(at) => Value::Dur(at.load(Ordering::Acquire)),
        }
    }

    fn is_atomic(&self) -> bool {
        if let ValueContainer::Str(_) = self {
            return false;
        }

        true
    }
}

pub(crate) struct Plugin {
    name: String,
    token: Token,
    channel: Sender<Message>
}

unsafe impl Send for Plugin {}

#[derive(Debug,Clone,PartialEq)]
pub(crate) struct Token {
    val: [u8;32]
}

impl Token {
    fn new() -> Token {
        let mut rng = Hc128Rng::from_rng(rand::thread_rng()).unwrap();
        let mut val = [0_u8; 32];
        rng.fill_bytes(&mut val);

        Token { val }
    }
}

impl Default for Token {
    fn default() -> Self {
        Token { val: [0_u8;32] }
    }
}
