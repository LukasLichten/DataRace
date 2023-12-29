use std::sync::{atomic::{AtomicU64, AtomicI64, AtomicBool}, Arc};
use std::hash::{Hasher,Hash};

use fnv::{FnvHashMap,FnvHasher};
use tokio::sync::{RwLock, Mutex};
use kanal::{Sender, Receiver};
use rand::{RngCore, SeedableRng};
use rand_hc::Hc128Rng;

use crate::{pluginloader::{Message, Value}, DataStoreReturnCode, PropertyHandle};

/// This is our centralized State
pub(crate) struct DataStore {
    // Definitly some optimizations can be made here
    property_map: RwLock<FnvHashMap<String, usize>>,
    propertys: RwLock<Vec<Property>>,
    plugins: RwLock<Vec<Plugin>>
}

impl DataStore {
    pub fn new() -> DataStore {
        DataStore {
            property_map: RwLock::new(FnvHashMap::default()), 
            propertys: RwLock::new(Vec::<Property>::new()),
            plugins: RwLock::new(Vec::<Plugin>::new())
        }
    }

    pub async fn create_property(&self, token: &Token, name: String, value: Value) -> Result<PropertyHandle,DataStoreReturnCode> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DataStoreReturnCode::AlreadyExists);
        }

        if let Some(plugin_name) = self.get_plugin_name_from_token(token).await {
            let (mut w_map, mut w_list) = (self.property_map.write().await, self.propertys.write().await);
            
            let property_name = format!("{}.{}", plugin_name, name);

            // Property seems to exist already...
            // Placeholders could have been created by listeners
            if let Some(address) = w_map.get(&property_name) {
                let address = address.clone();
                if let Some(item) = w_list.get_mut(address) {
                    if item.plugin_token == Token::default() && property_name == item.name {
                        // This is a placeholder, we replace the token, set Value, and ping
                        // listeners
                        
                        item.plugin_token = token.clone();
                        item.value = ValueContainer::new(&value);

                        drop(w_map);
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
                        panic!("Corruption in the datastore: map pointes to item of a different name");
                    }
                } else {
                    // Something is corrupted here
                    // TODO: We could fix it, removing the bad reference from the HashMap, but what
                    // if the item exists somewhere else? We would need to iterate through all
                    // properties to check
                    panic!("Corruption in the datastore: map contains pointer beyond array");
                }
            }

            // Normal case: We create a new Property, and insert it
            let mut hasher = FnvHasher::default();
            property_name.hash(&mut hasher);

            let name_hash = hasher.finish();

            w_list.push(Property { 
                name: property_name.clone(), 
                name_hash, 
                plugin_token: token.clone(), 
                listeners: vec![],
                value: ValueContainer::new(&value)
            });

            let index = w_list.len() - 1;

            w_map.insert(property_name, index);

            return Ok(PropertyHandle { index, hash: name_hash });
        } else {
            // Not Autheticated
            return Err(DataStoreReturnCode::NotAuthenticated);
        }
    }

    pub(crate) async fn update_property(&self, token: &Token, handle: &PropertyHandle, value: Value) -> DataStoreReturnCode {
        let r_list = self.propertys.read().await;

        if let Some(item) = r_list.get(handle.index.clone()) {
            if item.name_hash != handle.hash {
                return DataStoreReturnCode::OutdatedPropertyHandle;
            }

            if &item.plugin_token != token {
                return DataStoreReturnCode::NotAuthenticated;
            }

            if let Some(value) = item.value.update(value) {
                
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

    pub(crate) async fn change_property_name(&self, token: &Token, handle: &PropertyHandle, name: String) -> Result<PropertyHandle, DataStoreReturnCode> {
        todo!()
    }

    pub(crate) async fn change_property_type(&self, token: &Token, handle: &PropertyHandle, value: Value) -> DataStoreReturnCode {
        todo!();
    }

    pub(crate) async fn subscribe_to_property(&self, token: &Token, handle: &PropertyHandle) -> DataStoreReturnCode {
        todo!();
    }

    pub(crate) async fn unsubscribe_from_property(&self, token: &Token, handle: &PropertyHandle) -> DataStoreReturnCode {
        todo!();
    }

    pub(crate) async fn delete_property(&self, token: &Token, handle: &PropertyHandle) -> DataStoreReturnCode {
        let (mut w_map, mut w_list) = (self.property_map.write().await, self.propertys.write().await);
        
        if let Some(item) = w_list.get_mut(handle.index.clone()) {
            if item.name_hash != handle.hash {
                return DataStoreReturnCode::OutdatedPropertyHandle;
            }

            if &item.plugin_token != token {
                return DataStoreReturnCode::NotAuthenticated;
            }

            // Everything is authenticated
            w_map.remove(&item.name);

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

    pub(crate) async fn get_property_handle(&self, name: String) -> Result<PropertyHandle, DataStoreReturnCode> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err(DataStoreReturnCode::DoesNotExist);
        }

        let (r_map, r_list) = (self.property_map.read().await, self.propertys.read().await);
        
        if let Some(address) = r_map.get(&name) {
            let address = address.clone();

            if let Some(item) = r_list.get(address) {
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
        let r_list = self.propertys.read().await;

        if let Some(item) = r_list.get(handle.index.clone()) {
            if item.name_hash != handle.hash {
                return Err(DataStoreReturnCode::OutdatedPropertyHandle);
            }
            
            Ok(item.value.read())
        } else {
            Err(DataStoreReturnCode::OutdatedPropertyHandle)
        }
    }

    pub(crate) async fn create_plugin(&self, name: String) -> Option<(Token, Receiver<Message>)> {
        if name.trim().is_empty() {
            return None;
        }

        let mut w_plugin = self.plugins.write().await;
        
        let mut token = Token::default();
        while token == Token::default() {
            token = Token::new();

            for plugin in w_plugin.iter() {
                if plugin.token == token {
                    token = Token::default();
                    break;
                } else if name == plugin.name {
                    return None;
                }
            }
        }

        let (sx,rx) = kanal::unbounded();
        
        w_plugin.push(Plugin { name, token: token.clone(), channel: sx });

        Some((token, rx))
    }

    pub(crate) async fn delete_plugin(&self, token: &Token) -> DataStoreReturnCode {
        let mut w_plugin = self.plugins.write().await;

        for p in w_plugin.iter_mut() {
            if &p.token == token {
                // We can't remove a plugin as it would change the index of items
                //
                p.name = String::new();
                p.token = Token::default();
                
                //Can't unset this, but the channel should be closed automatically anyway
                //p.channel = None
                
                // We should also remove the subscription of every property, but that is way way to
                // expensive, and the delete_plugin should really only be called during crashes and shutdown
                // At least during shutdown this would be a massive waste of time
                
                return DataStoreReturnCode::Ok;
            }
        }

        DataStoreReturnCode::DoesNotExist
    } 
    
    /// Be careful, this can deadlock any caller who has taken a lock on plugins
    async fn get_plugin_name_from_token(&self, token: &Token) -> Option<String> {
        if token == &Token::default() {
            // default is reserved for none/owned by Datastore
            return None;
        }

        let r_plugins = self.plugins.read().await;

        // This is highly inefficient, but considering there is only like 2 dozend plugins this
        // should never be a problem
        // And we only need this when registering Plugins
        for p in r_plugins.iter() {
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

pub(crate) enum ValueContainer {
    None,
    Int(AtomicI64),
    Float(AtomicU64),
    Bool(AtomicBool),
    Str(Mutex<Arc<String>>),
    Dur(AtomicU64)
}

impl ValueContainer {
    fn new(val: &Value) -> ValueContainer {
        todo!()
    }

    fn update(&self, val: Value) -> Option<Value> {
        todo!()
    }

    fn read(&self) -> Value {
        todo!()
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
