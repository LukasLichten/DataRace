use std::{path::PathBuf, str::FromStr};

use log::info;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use kanal::{AsyncSender, Sender};
use hashbrown::HashMap;

use crate::{events::EventMessage, pluginloader::LoaderMessage, utils::{PluginStatus, ValueContainer}, web::SocketChMsg, DataStoreReturnCode, PluginHandle, PropertyHandle};

/// This is our centralized State
pub(crate) struct DataStore {
    plugins: HashMap<u64, Plugin>,
    // Serves for access by the websocket
    properties: HashMap<PropertyHandle, ValueContainer>,
    // As the hash is not reversible, but for certain opertations we need the name...
    prop_names: HashMap<PropertyHandle, String>,
    
    config: Config,
    
    // task_map: HashMap<tokio::task::Id, (u64, String)>,
    
    shutdown: bool,

    event_channel: kanal::Sender<EventMessage>,
    websocket_channel: kanal::AsyncSender<SocketChMsg>
}

impl DataStore {
    pub fn new(event_channel: kanal::Sender<EventMessage>, websocket_channel: kanal::AsyncSender<SocketChMsg>) -> RwLock<DataStore> {
        RwLock::new(DataStore {
            plugins: HashMap::default(),
            properties: HashMap::default(),
            prop_names: HashMap::default(),
            config: Config::default(),
            // task_map: HashMap::default(),
            shutdown: false,
            event_channel,
            websocket_channel
        })
    }

    pub(crate) fn register_plugin(&mut self, id: u64, sx: Sender<LoaderMessage>, handle: *mut PluginHandle) -> Option<()> {
        if self.shutdown {
            return None;
        }
        
        if self.plugins.contains_key(&id) {
            return None;
        } 

        self.plugins.insert(id, Plugin { channel: sx.to_async(), handle, plugin_status: PluginStatus::Init });
        Some(())
    }

    pub(crate) async fn delete_plugin(&mut self, id: u64, safe_shutdown: bool) -> DataStoreReturnCode {
        if self.plugins.contains_key(&id) {
            let handle = self.plugins[&id].handle;
            
            self.plugins.remove(&id);


            // Deallocating the pluginhandle, but only when we are sure it all correctly shut down
            if safe_shutdown {
                unsafe {
                    drop(Box::from_raw(handle));
                }
            }


            if self.shutdown {
                // short cut
                return DataStoreReturnCode::Ok;
            }

            // Deletes the properties of this plugin from the datastore,
            // so they won't be available to the web endpoint anymore
            self.properties.retain(|&k, _| k.plugin != id );

            let _ = self.event_channel.as_async().send(EventMessage::RemovePlugin(id));

            // TODO send a message to all other plugins so they can remove leftover
            // properties/subscriptions from
            // this plugin

            DataStoreReturnCode::Ok
        } else {
            DataStoreReturnCode::DoesNotExist
        }

    } 

    pub(crate) fn get_event_channel<'a>(&'a self) -> &'a Sender<EventMessage> {
        &self.event_channel
    }

    pub(crate) fn get_websocket_channel<'a>(&'a self) -> &'a AsyncSender<SocketChMsg> {
        &self.websocket_channel
    }

    pub(crate) fn count_plugins(&self) -> usize {
        self.plugins.iter().filter(|(_,p)|p.plugin_status == PluginStatus::Running).count()
    }

    pub(crate) async fn start_shutdown(&mut self) {
        info!("Beginning Shutdown... ");
        self.shutdown = true;

        for (_,plugin) in self.plugins.iter() {
            let _ = plugin.channel.send(LoaderMessage::Shutdown).await;
        }

        let _ = self.event_channel.as_async().send(EventMessage::Shutdown).await;
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

    /// This creates/replaces a properties value container
    /// There is no check if this plugin is allowed to edit this property, so use carefully
    pub(crate) async fn set_property(&mut self, handle: PropertyHandle, val: ValueContainer) {
        self.properties.insert(handle, val.shallow_clone());
        let _ = self.websocket_channel.send(SocketChMsg::ChangedProperty(handle, val)).await;
    }

    /// Serves for displaying the property name
    pub(crate) fn register_property_name(&mut self, handle: PropertyHandle, name: String) {
        self.prop_names.insert(handle, name);
    }

    /// Retrieves the property name
    pub(crate) fn read_property_name(&self, handle: &PropertyHandle) -> Option<String> {
        Some(self.prop_names.get(handle)?.clone())
    }

    /// Retrieves a reference to the valuecontainer (if present)
    /// There are again no checks if you (plugin etc) are allowed to edit this value, 
    /// so you should only read the values contained
    pub(crate) fn get_property_container<'a>(&'a self, handle: &PropertyHandle) -> Option<&'a ValueContainer> {
        self.properties.get(handle)
    }

    /// Deletes the Property (only if it exists) with no further checks
    pub(crate) async fn delete_property(&mut self, handle: &PropertyHandle) {
        self.properties.remove(handle);
        let _ = self.websocket_channel.send(SocketChMsg::ChangedProperty(handle.clone(), ValueContainer::None)).await;
    }

    pub(crate) fn count_properties(&self) -> usize {
        self.properties.iter().count()
    }


    pub(crate) fn get_config<'a>(&'a self) -> &'a Config {
        &self.config
    }

    pub(crate) fn iter_properties<'a>(&'a self) -> hashbrown::hash_map::Keys<'a, PropertyHandle, ValueContainer> {
        self.properties.keys()
    }

    pub(crate) async fn set_plugin_ready(&mut self, id: u64) {
        if let Some(p) = self.plugins.get_mut(&id) {
            p.plugin_status = PluginStatus::Running;
        } else {
            // This should not happen... ever
            panic!("A plugin was attempted to be set ready... that doesn't exist: {id}")
        }

        for (other_id, _) in self.plugins.iter().filter(|(k,p)| p.plugin_status == PluginStatus::Running && **k != id) {
            // Inform plugin of ours running
            self.send_message_to_plugin(*other_id, LoaderMessage::OtherPluginStartup(id)).await;

            // Inform our plugin of the plugin that is already running
            self.send_message_to_plugin(id, LoaderMessage::OtherPluginStartup(*other_id)).await;
        }
    }
}

pub(crate) struct Plugin {
    channel: AsyncSender<LoaderMessage>,
    handle: *mut PluginHandle,
    plugin_status: PluginStatus
}

unsafe impl Send for Plugin {}
unsafe impl Sync for Plugin {}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) struct Config {
    plugin_location: PathBuf,
    dashboards_location: PathBuf
}

impl Default for Config {
    fn default() -> Self {
        let base = PathBuf::from_str(".").expect("Current folder dereference should always work");
        Config {
            plugin_location: {
                let mut plugin = base.clone();
                plugin.push("plugins");
                plugin
            },
            dashboards_location: {
                let mut dash = base.clone();
                dash.push("dashboards");
                dash
            },
        }
    }
}

impl Config {
    pub(crate) fn get_plugin_folder(&self) -> PathBuf {
        self.plugin_location.clone()
    }


    pub(crate) fn get_dashboards_folder(&self) -> PathBuf {
        self.dashboards_location.clone()
    }
}
