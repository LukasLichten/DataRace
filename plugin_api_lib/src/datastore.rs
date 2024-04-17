use std::sync::{atomic::{AtomicU64, AtomicI64, AtomicBool, Ordering}, Arc};
use std::hash::{Hasher,Hash};

use log::{error, info};
use tokio::sync::{RwLock, Mutex};
use kanal::{AsyncSender, Receiver, Sender};
use rand::{RngCore, SeedableRng};
use rand_hc::Hc128Rng;
use hashbrown::HashMap;

use crate::{pluginloader::LoaderMessage, utils::Value, DataStoreReturnCode, PluginHandle, PropertyHandle};

/// This is our centralized State
pub(crate) struct DataStore {
    // Definitly some optimizations can be made here
    plugins: HashMap<u64, Plugin>,
    shutdown: bool
}

impl DataStore {
    pub fn new() -> RwLock<DataStore> {
        RwLock::new(DataStore {
            plugins: HashMap::default(),
            shutdown: false
        })
    }

    // pub(crate) fn create_plugin(&mut self, name: String) -> Option<(Token, Receiver<Message>, Sender<Message>)> {
    //     let (sx,rx) = kanal::unbounded();
    //     
    //     let token = self.register_plugin(name, sx.clone())?;
    //
    //     Some((token, rx, sx))
    // }

    /// Reuses an existing channel
    pub(crate) fn register_plugin(&mut self, id: u64, sx: Sender<LoaderMessage>, handle: *mut PluginHandle) -> Option<()> {
        if self.shutdown {
            return None;
        }
        
        if self.plugins.contains_key(&id) {
            return None;
        } 

        self.plugins.insert(id, Plugin { channel: sx.to_async(), handle });
        Some(())
    }

    pub(crate) async fn delete_plugin(&mut self, id: u64) -> DataStoreReturnCode {
                
        if self.plugins.contains_key(&id) {
            let handle = self.plugins[&id].handle;
            
            self.plugins.remove(&id);


            // Deallocating the pluginhandle
            unsafe {
                drop(Box::from_raw(handle));
            }


            if self.shutdown {
                // short cut
                return DataStoreReturnCode::Ok;
            }

            // TODO send a message to all other plugins so they can remove leftover
            // properties/subscriptions from
            // this plugin

            DataStoreReturnCode::Ok
        } else {
            DataStoreReturnCode::DoesNotExist
        }

    } 

    pub(crate) async fn start_shutdown(&mut self) {
        info!("Beginning Shutdown... ");
        self.shutdown = true;

        for (_,plugin) in self.plugins.iter() {
            let _ = plugin.channel.send(LoaderMessage::Shutdown).await;
        }
    }

    pub(crate) fn get_shutdown_status(&self) -> bool {
        self.shutdown
    }

    pub(crate) async fn send_message_to_plugin(&self, id: u64, msg: LoaderMessage) -> bool {
        if let Some(plugin) = self.plugins.get(&id) {
            plugin.channel.send(msg).await.is_ok()
        } else {
            false
        }
    }
}

pub(crate) struct Plugin {
    channel: AsyncSender<LoaderMessage>,
    handle: *mut PluginHandle
}

unsafe impl Send for Plugin {}
unsafe impl Sync for Plugin {}

