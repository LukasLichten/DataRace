use std::{collections::{hash_map, HashMap}, ffi::CString, path::PathBuf, sync::atomic::AtomicU64};

use log::info;
use rand::Rng;
use tokio::{io::AsyncWriteExt, sync::RwLock};
use kanal::{AsyncSender, Sender};

use datarace_socket_spec::socket::Action as WebAction;

use crate::{events::EventMessage, plattform::PluginSettingsFile, pluginloader::LoaderMessage, utils::{self, PluginStatus, ValueContainer, U256}, web::{IpMatcher, SocketChMsg}, Action, DataStoreReturnCode, PluginHandle, PluginSettingsLoadFail, PluginSettingsLoadReturn, PluginSettingsLoadState, PropertyHandle};

/// This is our centralized State
pub(crate) struct DataStore {
    errors: bool,

    plugins: HashMap<u64, Plugin>,
    // Serves for access by the websocket
    properties: HashMap<PropertyHandle, ValueContainer>,
    // As the hash is not reversible, but for certain opertations we need the name...
    prop_names: HashMap<PropertyHandle, String>,
    
    config: Config,
    
    // task_map: HashMap<tokio::task::Id, (u64, String)>,
    next_action_id: AtomicU64,
    
    shutdown: bool,

    event_channel: kanal::Sender<EventMessage>,
    websocket_channel: kanal::AsyncSender<SocketChMsg>,

    // Secrets
    pub(crate) dashboard_hasher_secret: U256
}

impl DataStore {
    pub fn new(event_channel: kanal::Sender<EventMessage>, websocket_channel: kanal::AsyncSender<SocketChMsg>, config: Config, errors: bool) -> RwLock<DataStore> {
        let mut ran = rand::rng();
        let secret: [u64;4] = ran.random();

        RwLock::new(DataStore {
            errors,
            plugins: HashMap::default(),
            properties: HashMap::default(),
            prop_names: HashMap::default(),
            config,
            // task_map: HashMap::default(),
            shutdown: false,
            next_action_id: AtomicU64::new(0),

            event_channel,
            websocket_channel,

            dashboard_hasher_secret: U256(secret)
        })
    }

    pub(crate) async fn register_plugin(&mut self, id: u64, plugin_type: PluginType) -> Option<PluginSettingsLoadReturn> {
        if self.shutdown {
            return None;
        }
        
        if self.plugins.contains_key(&id) {
            return None;
        } 

        match &plugin_type {
            PluginType::Internal { handle, channel: _ } => {
                if handle.is_null() {
                    return None;
                }
            }
        }

        // This would handle the edge case of settings properties of a plugin being loaded before
        // the plugin, but this is impossible. In case this becomes possible, reactivate this line.
        //
        // Also, despite the settings getting loaded after this, because register_plugin requires
        // mutable lock, the socket can't read till the plugin is inserted.
        // let _ = self.websocket_channel.send(SocketChMsg::ReloadedPluginSettings(id)).await;

        let (plugin, settings_load_state) = Plugin::new(plugin_type, &self.config).await;
        self.plugins.insert(id, plugin);

        Some(settings_load_state)
    }

    pub(crate) async fn delete_plugin(&mut self, id: u64, safe_shutdown: bool) -> DataStoreReturnCode {
        if self.plugins.contains_key(&id) {
            let handle = match self.plugins[&id].value {
                PluginType::Internal { handle, channel: _ } => Some(handle),
                // _ => None
            };
            
            self.plugins.remove(&id);


            if let Some(handle) = handle {
                // Deallocating the pluginhandle, but only when we are sure it all correctly shut down
                if safe_shutdown {
                    unsafe {
                        drop(Box::from_raw(handle));
                    }
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

    /// It is expected the action has already the correct action, origin and the params are insured to be
    /// correctly formated.
    /// The id is automatically set here
    pub(crate) async fn trigger_action(&self, target_plugin: u64, mut action: Action) -> Option<u64> {
        let id = self.next_action_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        action.id = id;

        match self.plugins.get(&target_plugin).map(|p| &p.value) {
            Some(PluginType::Internal { handle: _, channel }) => {
                if channel.send(LoaderMessage::Action(action)).await.is_ok() {
                    return Some(id);
                }
            },
            None => ()
        }

        None
    }

    /// This is for handling actions coming in from the websocket.
    /// We only convert if the plugin is a native plugin, and then pass on our event
    pub(crate) async fn trigger_web_action(&self, origin: u64, action: WebAction) -> Result<u64, &'static str> {
        let id = self.next_action_id.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let action_handle: crate::ActionHandle = action.action.try_into().map_err(|_| "Malformed ActionHandle")?;

        match self.plugins.get(&action_handle.plugin).map(|p| &p.value) {
            Some(PluginType::Internal { handle: _, channel }) => {
                let (params, param_count) = utils::web_vec_to_c_array(action.param)?;

                let mut action = Action::new(origin, action_handle.action, params, param_count);
                action.id = id;

                if channel.send(LoaderMessage::Action(action)).await.is_ok() {
                    return Ok(id);
                }
            },
            None => ()
        }

        Err("Plugin does not exist")
    }

    /// It is expected the action/return code, id, origin and param are set and correctly formated.
    /// Nothing is done besides sending it.
    pub(crate) async fn callback_action(&self, target_plugin: u64, callback: Action) -> bool {
        match self.plugins.get(&target_plugin).map(|p| &p.value) {
            Some(PluginType::Internal { handle: _, channel }) => {
                channel.send(LoaderMessage::ActionCallback(callback)).await.is_ok()
            },
            None => false,
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
            match &plugin.value {
                PluginType::Internal { handle: _, channel } => {
                    let _ = channel.send(LoaderMessage::Shutdown).await;
                },
            }
        }

        let _ = self.event_channel.as_async().send(EventMessage::Shutdown).await;
    }

    pub(crate) fn get_shutdown_status(&self) -> bool {
        self.shutdown
    }

    pub(crate) async fn send_message_to_plugin(&self, id: u64, msg: LoaderMessage) -> bool {
        match self.plugins.get(&id).map(|p| &p.value) {
            Some(PluginType::Internal { handle: _, channel }) => {
                channel.send(msg).await.is_ok()
            },
            None => false
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

    pub(crate) fn iter_properties<'a>(&'a self) -> hash_map::Keys<'a, PropertyHandle, ValueContainer> {
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

    pub(crate) fn get_plugin_settings_property<'a>(&'a self, plugin: u64, property: u64) -> Option<&'a PluginSettingProperty> {
        Some(self.plugins.get(&plugin)?.settings.get(&property)?)
    }

    /// No checks if the property already exists, and will just replace the value instead
    pub(crate) async fn insert_plugin_settings_property(&mut self, plugin: u64, property: u64, prop: PluginSettingProperty) -> DataStoreReturnCode {
        match self.plugins.get_mut(&plugin) {
            Some(plugin_cont) => {
                let value = prop.value.shallow_clone();
                plugin_cont.settings.insert(property, prop);
                if self.websocket_channel.send(SocketChMsg::ChangedSettingsProperty(PropertyHandle { plugin, property }, value)).await.is_ok() {
                    DataStoreReturnCode::Ok
                } else {
                    DataStoreReturnCode::InternalError
                }
            },
            None => {
                DataStoreReturnCode::InternalError
            }
        }
    }

    pub(crate) fn set_plugin_settings_property_transient(&mut self, plugin: u64, property: u64, transient: bool) -> DataStoreReturnCode {
        match self.plugins.get_mut(&plugin) {
            Some(plugin) => {
                match plugin.settings.get_mut(&property) {
                    Some(value) => {
                        value.transient = transient;
                        DataStoreReturnCode::Ok
                    },
                    None => DataStoreReturnCode::DoesNotExist
                }
            },
            None => DataStoreReturnCode::InternalError
        }
    }

    pub(crate) async fn remove_plugin_settings_property(&mut self, plugin: u64, property: u64) -> DataStoreReturnCode {
        match self.plugins.get_mut(&plugin) {
            Some(plugin_cont) => {
                match plugin_cont.settings.remove(&property) {
                    Some(_) => {
                        if self.websocket_channel.send(SocketChMsg::ChangedSettingsProperty(PropertyHandle { plugin, property }, ValueContainer::None)).await.is_ok() {
                            DataStoreReturnCode::Ok
                        } else {
                            DataStoreReturnCode::InternalError
                        }
                    },
                    None => DataStoreReturnCode::DoesNotExist
                }
            },
            None => DataStoreReturnCode::InternalError
        }
    }

    pub(crate) async fn save_plugin_settings(&self, plugin: u64) -> PluginSettingsLoadState {
        match self.plugins.get(&plugin) {
            Some(plugin) => {
                match plugin.save_settings(&self.config).await {
                    Ok(()) => PluginSettingsLoadState::Loaded,
                    Err(e) => e
                }
            },
            None => {
                // This should not happen
                PluginSettingsLoadState::PslsHandleNullPtr
            }
        }
    }

    pub(crate) async fn reload_plugin_settings(&mut self, plugin: u64) -> PluginSettingsLoadReturn {
        match self.plugins.get_mut(&plugin) {
            Some(plugin_cont) => {
                let res = plugin_cont.reload_settings(&self.config).await;

                // Iterating through all settings and informing the websocket
                let _ = self.websocket_channel.send(SocketChMsg::ReloadedPluginSettings(plugin));


                res
            },
            None => {
                // This should not happen
                PluginSettingsLoadReturn { code: PluginSettingsLoadState::PslsHandleNullPtr, fail: PluginSettingsLoadFail { filler: 0 } }
            }
        }
    }

    /// Returns if any internal errors occured
    ///
    /// This is currently mainly config errors or other severe behavior alteration (excluding plugins)
    pub(crate) fn has_errors(&self) -> bool {
        self.errors
    }


    /// Sets that an error has occured
    ///
    /// This is only for configuration errors or failed services (not for plugins and random bad web requests)
    pub(crate) fn set_errors(&mut self) {
        self.errors = true;
    }
}

#[macro_export]
macro_rules! set_errors {
    ($ds:ident) => {
        let mut ds_w = $ds.write().await;
        ds_w.set_errors();
        drop(ds_w);
    };
}

pub(crate) struct PluginSettingProperty {
    pub name: String,
    pub value: ValueContainer,
    pub transient: bool
}

struct Plugin {
    value: PluginType,
    settings: HashMap<u64, PluginSettingProperty>,
    plugin_status: PluginStatus
}

impl Plugin {
    async fn new(value: PluginType, config: &Config) -> (Plugin, PluginSettingsLoadReturn) {
        let settings = HashMap::new();
        let mut plugin = Plugin { value, settings, plugin_status: PluginStatus::Init };
        
        let res = plugin.reload_settings(config).await;

        (plugin, res)
    }

    async fn reload_settings(&mut self, config: &Config) -> PluginSettingsLoadReturn {
        let mut file  = config.plugin_settings_location.clone();
        file.push(format!("{}.json", self.value.get_name().to_lowercase()));

        // Reading and parsing the file
        let res = match tokio::fs::read_to_string(file).await {
            Ok(res) => res,
            Err(e) => {
                match e.kind() {
                    std::io::ErrorKind::NotFound => 
                        return PluginSettingsLoadReturn { code: PluginSettingsLoadState::NoFile, fail: PluginSettingsLoadFail { filler: 0 } },
                    _ => {
                        let failure = CString::new(e.to_string()).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut());
                        return PluginSettingsLoadReturn { code: PluginSettingsLoadState::FileSystemError, fail: PluginSettingsLoadFail { text: failure  } }
                    }
                };
            }
        };

        let data: PluginSettingsFile = match serde_json::from_str(res.as_str()) {
            Ok(data) => data,
            Err(e) => {
                let failure = CString::new(e.to_string()).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut());
                return PluginSettingsLoadReturn { code: PluginSettingsLoadState::JsonParseError, fail: PluginSettingsLoadFail { text: failure  } }
            }
        };


        // Loading Settings
        self.settings.clear();
        for (key, value) in data.settings {
            let (id,cont) = match (utils::generate_property_name_hash(key.as_str()),ValueContainer::new_web(value)) {
                (Some(id),Some(cont)) => (id, cont),
                (None, Some(_)) => {
                    self.settings.clear();

                    let failure = CString::new(format!("Property Name Error: '{}' is an invalid name", key)).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut());
                    return PluginSettingsLoadReturn { code: PluginSettingsLoadState::JsonParseError, fail: PluginSettingsLoadFail { text: failure  } }
                },
                (Some(_), None) => {
                    self.settings.clear();

                    let failure = CString::new(format!("Invalid Property Type Error: Property '{}' can not be ArrUpdate", key)).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut());
                    return PluginSettingsLoadReturn { code: PluginSettingsLoadState::JsonParseError, fail: PluginSettingsLoadFail { text: failure  } }
                },
                (None, None) => {
                    self.settings.clear();

                    let failure = CString::new(format!("Property Error: Property '{}' has an invalid type and name", key)).map(|c| c.into_raw()).unwrap_or(std::ptr::null_mut());
                    return PluginSettingsLoadReturn { code: PluginSettingsLoadState::JsonParseError, fail: PluginSettingsLoadFail { text: failure  } }
                }
            };

            let val = PluginSettingProperty { name: key, transient: false, value: cont };
            self.settings.insert(id, val);
        }


        // Informing in case of version change of the plugin since the settings were last saved
        match utils::compare_version_numbers(&self.value.get_version(), &data.version) {
            std::cmp::Ordering::Equal => {
                PluginSettingsLoadReturn { code: PluginSettingsLoadState::Loaded, fail: PluginSettingsLoadFail { filler: 0 } }
            },
            std::cmp::Ordering::Less => {
                // Plugin version is older then Config version
                // Meaning the user rolled back the plugin version
                PluginSettingsLoadReturn { code: PluginSettingsLoadState::VersionNewerThenCurrent, fail: PluginSettingsLoadFail { version: data.version } }
            }
            std::cmp::Ordering::Greater => {
                // Plugin version is newer then Config version
                PluginSettingsLoadReturn { code: PluginSettingsLoadState::VersionOlderThenCurrent, fail: PluginSettingsLoadFail { version: data.version } }
            }
        }
    }

    async fn save_settings(&self, config: &Config) -> Result<(), PluginSettingsLoadState> {
        let mut file  = config.plugin_settings_location.clone();
        file.push(format!("{}.json", self.value.get_name().to_lowercase()));

        let mut data = PluginSettingsFile { version: self.value.get_version(), settings: HashMap::default() };

        let mut cache = utils::ValueCache::default();
        for (_, item) in &self.settings {
            if !item.transient {
                item.value.read_web(&mut cache);

                data.settings.insert(item.name.clone(), std::mem::replace(&mut cache.value, datarace_socket_spec::socket::Value::None));
            }
        }

        let mut writer = tokio::fs::File::options().create(true).write(true).truncate(true).open(file).await.map_err(|_| PluginSettingsLoadState::FileSystemError)?;
        
        let res = serde_json::to_vec_pretty(&data).map_err(|_| PluginSettingsLoadState::JsonParseError)?;
        writer.write_all(res.as_slice()).await.map_err(|_| PluginSettingsLoadState::FileSystemError)?;

        Ok(())
    }
}

pub(crate) enum PluginType {
    Internal{ handle: *mut PluginHandle, channel: AsyncSender<LoaderMessage> }
}

impl PluginType {
    fn get_name(&self) -> String {
        match self {
            Self::Internal { handle, channel: _ } => {
                unsafe { handle.as_ref() }.expect("Within Datastore PluginHandle should never be null").name.clone()
            }
        }
    }

    fn get_version(&self) -> [u16;3] {
        match self {
            Self::Internal { handle, channel: _ } => {
                unsafe { handle.as_ref() }.expect("Within Datastore PluginHandle should never be null").version.clone()
            }
        }
    }
}

unsafe impl Send for PluginType {}
unsafe impl Sync for PluginType {}

#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub(crate) disable_web_server: bool,
    pub(crate) web_server_ip: String,
    pub(crate) web_server_port: u16,
    pub(crate) web_ip_whitelist: Option<IpMatcher>,
    pub(crate) web_settings_whitelist: Option<IpMatcher>,

    pub(crate) plugin_locations: Vec<PathBuf>,
    pub(crate) dashboards_location: PathBuf,
    pub(crate) plugin_settings_location: PathBuf
}

