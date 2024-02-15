use std::sync::{atomic::{AtomicU64, AtomicI64, AtomicBool, Ordering}, Arc};
use std::hash::{Hasher,Hash};

use fnv::{FnvHashMap,FnvHasher};
use log::{error, info};
use tokio::sync::{RwLock, Mutex};
use kanal::{AsyncSender, Receiver, Sender};
use rand::{RngCore, SeedableRng};
use rand_hc::Hc128Rng;

use crate::{pluginloader::Message, DataStoreReturnCode, PropertyHandle, utils::Value};

/// This is our centralized State
pub(crate) struct DataStore {
    // Definitly some optimizations can be made here
    property_map: FnvHashMap<String, usize>,
    propertys: Vec<Property>,
    plugins: Vec<Plugin>,
    shutdown: bool
}

impl DataStore {
    pub fn new() -> RwLock<DataStore> {
        RwLock::new(DataStore {
            property_map: FnvHashMap::default(), 
            propertys: Vec::<Property>::new(),
            plugins: Vec::<Plugin>::new(),
            shutdown: false
        })
    }

    #[allow(unreachable_code)]
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

                        todo!("Placeholder handling");
                        // for p in item.listeners.iter() {
                        //     // Out creation could be stalled by a full message queue...
                        //     let _ = p.as_async().send(Message {}).await;
                        // }

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

            if item.value.update(value, handle).await {
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

            if let ValueContainer::Str(ref _mu, ref mut listeners) = item.value {
                // Strings have special listeners that we subscribe to this way
                for (_, old_token) in listeners.iter() {
                    if old_token == token {
                        // We return OK, as with collisions in none strings we don't know if they
                        // collide.
                        // But I guess, the user requested to be subscribed, and is still
                        // subscribed afterwards, so I guess okay
                        return DataStoreReturnCode::Ok;
                    }
                }
                for p in self.plugins.iter() {
                    if &p.token == token {
                        listeners.push((p.channel.clone().to_async(),token.clone()));
                        return DataStoreReturnCode::Ok
                    }
                }

                DataStoreReturnCode::NotAuthenticated
            } else {
                // We are sending a message into the pluginmanager to poll this property, as this
                // is more performant
                for p in self.plugins.iter() {
                    if &p.token == token {
                        if p.channel.as_async().send(Message::Subscribe(handle.clone())).await.is_ok() {
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

            if let ValueContainer::Str(ref _mu, ref mut listeners) = item.value {
                let mut index = 0;
                while index < listeners.len() {
                    let (_sub, ls_token) = &listeners[index];
                    if ls_token == token {
                        break;
                    }

                    index += 1;
                }

                if index < listeners.len() {
                    listeners.remove(index);
                }

                // Yes, this means even when we didn't subscribe through not being subscribed we
                // return OK, but this is in line with polled values (where we don't know what the
                // pluginhandler state is or anything)
                //
                // Also this is fine, as now we are unsubscribed, it just was unnecessary
                return DataStoreReturnCode::Ok;
            }

            for p in self.plugins.iter() {
                if &p.token == token {
                    let _ = p.channel.as_async().send(Message::Unsubscribe(handle.clone())).await;
                    
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
            if let ValueContainer::Str(_mu, listeners) = &item.value {
                // In case of string we have to inform everyone that the property was deleted

                for (sub,_) in listeners.iter() {
                    let _ = sub.send(Message::Removed(handle.clone()) ).await;
                }
            }

            item.value = ValueContainer::None;

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
        let (sx,rx) = kanal::unbounded();
        
        let token = self.register_plugin(name, sx.clone())?;

        Some((token, rx, sx))
    }

    /// Reuses an existing channel
    pub(crate) fn register_plugin(&mut self, name: String, sx: Sender<Message>) -> Option<Token> {
        if self.shutdown {
            return None;
        }

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

        self.plugins.push(Plugin { name, token: token.clone(), channel: sx });
        Some(token)
    }

    pub(crate) async fn delete_plugin(&mut self, token: &Token) -> DataStoreReturnCode {
        for p in self.plugins.iter_mut() {
            if &p.token == token {
                // We can't remove a plugin as it would change the index of items
                p.name = String::new();
                p.token = Token::default();
                
                //Can't unset this, but the channel should be closed automatically anyway
                //p.channel = None

                if self.shutdown {
                    // short cut
                    return DataStoreReturnCode::Ok;
                }

                // Need to delete all properties tied to this plugins
                for index in 0..self.propertys.len() {
                    if let Some(item) = self.propertys.get_mut(index) {
                        if &item.plugin_token == token {
                            // debug!("Cleaning up property {} with value {}", &item.name, if let ValueContainer::Int(i) = &item.value { i.load(Ordering::Acquire).to_string() } else { "Error".to_string() });
                            let prop_handle = PropertyHandle { index, hash: item.name_hash };

                            self.property_map.remove(&item.name);

                            item.name = String::new();
                            item.name_hash = 0;
                            item.plugin_token = Token::default();
                            if let ValueContainer::Str(_mu, listeners) = &item.value {
                                // In case of string we have to inform everyone that the property was deleted

                                for (sub,_) in listeners.iter() {
                                    let _ = sub.send(Message::Removed(prop_handle.clone())).await;
                                }
                            }
                            item.value = ValueContainer::None;
                        } else if let ValueContainer::Str(ref _mu, ref mut listeners) = item.value {
                            // We also unsubscribe from the propertys that are type string
                            
                            let mut index = 0;
                            while index < listeners.len() {
                                let (_sub, ls_token) = &listeners[index];
                                if ls_token == token {
                                    break;
                                }

                                index += 1;
                            }

                            if index < listeners.len() {
                                listeners.remove(index);
                            }
                            
                            
                        }
                    }
                }
                
                return DataStoreReturnCode::Ok;
            }
        }

        DataStoreReturnCode::DoesNotExist
    } 

    pub(crate) async fn get_plugin_channel(&self, name: &String) -> Option<AsyncSender<Message>> {
        for item in self.plugins.iter() {
            if &item.name == name {
                return Some(item.channel.as_async().clone());
            }
        }

        None
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

    pub(crate) async fn start_shutdown(&mut self) {
        info!("Beginning Shutdown... ");
        self.shutdown = true;

        for plugin in self.plugins.iter() {
            let _ = plugin.channel.as_async().send(Message::Shutdown).await;
        }
    }

    pub(crate) fn get_shutdown_status(&self) -> bool {
        self.shutdown
    }
}

pub(crate) struct Property {
    name: String,
    name_hash: u64,
    plugin_token: Token,
    value: ValueContainer
}

unsafe impl Send for Property {}

#[derive(Debug)]
pub(crate) enum ValueContainer {
    None,
    Int(AtomicI64),
    Float(AtomicU64),
    Bool(AtomicBool),
    Str(Mutex<Arc<String>>, Vec<(AsyncSender<Message>, Token)>),
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
