use std::{collections::HashMap, fs::{self, ReadDir}, mem::ManuallyDrop, path::PathBuf};

use dlopen2::wrapper::{WrapperApi, Container};
use log::{error, info, debug};

use tokio::task::JoinSet;

use crate::{api_types, datastore::DataStore, events::EventMessage, set_errors, utils::{self, VoidPtrWrapper}, Action, DataStoreReturnCode, EventHandle, Message, MessagePtr, MessageType, MessageValue, PluginHandle, PropertyHandle};


struct PluginLocationIter {
    config_locations: Vec<PathBuf>,
    index: usize,
    directory: Option<ReadDir>,
}

impl Iterator for PluginLocationIter {
    type Item = Result<PathBuf, String>;

    fn next(&mut self) -> Option<Self::Item> {
        fn validate_path(path: PathBuf) -> Option<PathBuf> {
            debug!("Found in Plugin location: {}", path.to_str().unwrap_or_default());
            if !path.is_file() {
                return None;
            }

            let ending = if cfg!(target_os = "linux") {
                "so"
            } else {
                "dll"
            };

            if path.extension().unwrap_or_default().to_str().unwrap_or_default() == ending {
                Some(path)
            } else {
                None
            }
        }

        if let Some(dir) = &mut self.directory {
            // Folders are iterated till empty
            match dir.next() {
                Some(Ok(entry)) => {
                    if let Some(p) = validate_path(entry.path()) {
                        Some(Ok(p))
                    } else {
                        self.next()
                    }
                },
                Some(Err(e)) => Some(Err(e.to_string())),
                None => {
                    self.directory = None;
                    self.next()
                }
            }
        } else {
            // Handling Location Entries

            let item = self.config_locations.get(self.index)?;
            self.index += 1;
            if item.is_file() {
                if let Some(p) = validate_path(item.clone()) {
                    Some(Ok(p))
                } else {
                    self.next()
                }
            } else if item.is_dir() {
                debug!("Found Plugin Folder {}", item.to_str().unwrap_or_default());
                match fs::read_dir(item.as_path()) {
                    Ok(dir) => {
                        self.directory = Some(dir);
                        self.next()
                    },
                    Err(e) => Some(Err(format!("Unable to read folder {e}")))
                }
            } else {
                Some(Err(format!("{} Does Not Exist!", item.to_str().unwrap_or_default())))
            }
            
            
        }
    }
}


pub(crate) async fn load_all_plugins(datastore: &'static tokio::sync::RwLock<DataStore>) -> Result<JoinSet<Result<(),String>>,Box<dyn std::error::Error>> {
    let (plugin_locations, event_channel) = {
        let ds_r = datastore.read().await;
        (ds_r.get_config().plugin_locations.clone(), ds_r.get_event_channel().clone())
    };

    let mut iter = PluginLocationIter { config_locations: plugin_locations, index: 0, directory: None };

    let mut plugin_task_handles = JoinSet::<Result<(), String>>::new();

    while let Some(item) = iter.next() {
        match item {
            Ok(plugin) => {
                debug!("Loading Plugin...");
                let event_c = event_channel.clone();
                plugin_task_handles.spawn(run_plugin(plugin, datastore, event_c));
            },
            Err(e) => {
                error!("Unable to load: {e}");
                set_errors!(datastore);
            }
        }
    }


 
    Ok(plugin_task_handles)
}

/// https://www.man7.org/linux/man-pages/man3/dlopen.3.html
/// Flags passed in during dlopen:
/// Main flags (one has to be chosen):
/// - RTLD_LAZY: Resolves symbols only when called (default)
/// - RTLD_NOW: Resolves symbols before load finishes
/// Optional Flags (zero or more):
/// - RTLD_GLOBAL: Symbols defined by this shared object are available for symbol resolution by
/// others loaded later
/// - RTLD_LOCAL: default, symbols are not available for later objects
/// - RTLD_NODELETE: does not unload the object when getting dropped
/// - RTLD_NOLOAD: only opens if the object was loaded previously (irrelevant for us)
/// - RTLD_DEEPBIND: Place the lookup scope of the symbols in this shared object ahead of the global scope,
/// idk, probably means symbols would be loaded from here before resolving them elsewhere, allowing
/// overriding things. Not sure if this could be useful
const DLOPEN_FLAGS: Option<i32> = Some(libc::RTLD_NOW | libc::RTLD_LOCAL);

async fn run_plugin(path: PathBuf, datastore: &'static tokio::sync::RwLock<DataStore>, event_channel: kanal::Sender<EventMessage>) -> Result<(), String> {
    if let Ok(wrapper) = unsafe { Container::<PluginWrapper>::load_with_flags(path.to_str().unwrap(), DLOPEN_FLAGS) } {
        // When crashing out we want to avoid the unloading of the plugin, as this could cause as a
        // segfault in the still running thread
        let mut wrapper = ManuallyDrop::new(wrapper);

        // Preperations
        let desc = wrapper.get_plugin_description();

        let name = if let Some(n) = utils::get_string(desc.name) {
            wrapper.free_string(desc.name);
            n
        } else {
            error!("Unable to parse plugin name, id {}", desc.id);
            wrapper.free_string(desc.name);
            return Err(path.file_name().unwrap_or_default().to_str().unwrap_or_default().to_string());
        };

        if desc.api_version == u64::MAX {
            // Missmatched API Version
            error!("API version must be set at compiletime of your plugin ({}), requesting a API Version from the library during runtime will only return u64::MAX (DataRace is running on api version {})",
                name.as_str(), crate::API_VERSION);
            return Err(name);
        }
        if desc.api_version != crate::API_VERSION {
            // Missmatched API Version
            error!("Missmatched api version for plugin {}, will not be launched: Build for api {} (DataRace is running on {})", name.as_str(), desc.api_version, crate::API_VERSION);
            return Err(name);
        }

        // Verifying ID is generated correctly with hash
        let id = if let Some(id) = utils::generate_plugin_name_hash(name.as_str()) {
            if id != desc.id {
                error!("Plugin id set by plugin {} does not match the id generated by the name: Given {}, Expected: {}", name.as_str(), desc.id, id);
                return Err(name);
            }
            id
        } else {
            error!("Unable to verify plugin id set by plugin {}: plugin name does not comply with naming schema", name.as_str());
            return Err(name);
        };

        // Creates PluginHandle
        let (sender, receiver) = utils::get_message_channel();
        let handle = PluginHandle::new(name, id, datastore, sender.clone(), wrapper.free_string.clone(), desc.version, event_channel);
        let mut ptr_h = PtrWrapper { ptr: Box::into_raw(Box::new(handle)), is_locked: false, subscribers: HashMap::default() };
        drop(desc); // drop is importantent, name ptr is pointing at freed memory

        let mut w_store = datastore.write().await;
        if w_store.register_plugin(id, sender.clone(), ptr_h.ptr).is_none() {
            let name = get_plugin_name(&ptr_h);

            // We can drop the pointer with no risk, as nothing can access it
            unsafe {
                drop(Box::from_raw(ptr_h.ptr));
            }

            if w_store.get_shutdown_status() {
                error!("Unable to register Plugin {}, shut down already in progress", name.as_str());
                return Ok(());
            }

            error!("Unable to register Plugin {} (id {}), name/id collision", name.as_str(), id);
            return Err(name);
        }
        drop(w_store);

        if let Some(han) = unsafe {
            ptr_h.ptr.as_ref()    
        } {
            info!("Plugin {} (version {}.{}.{}) loaded", han.name, han.version[0], han.version[1], han.version[2]);
            debug!("Plugin {} has id {}", han.name, id);
        }

        // Safe shutdown is a flag to secure if we can be reasonable sure no other resource is
        // accessing the pluginhandle right now (like a thread spun up by the plugin)
        let mut safe_shutdown = false;

        // Initializing
        if wrapper.init(ptr_h.ptr) != 0 {
            // None Zero Error Code, shut down
            let name = get_plugin_name(&ptr_h);
            error!("Plugin {} failed to initialize", name.as_str());
            
            let mut w_store = datastore.write().await;
            let _ = w_store.delete_plugin(id, safe_shutdown).await;
            drop(w_store);

            return Err(name);
        } else if let Some(han) = unsafe { ptr_h.ptr.as_ref() } {
            let _ = han.sender.as_async().send(LoaderMessage::StartupFinished).await;
        }

        let async_rec = receiver.to_async();

        // let _ = sender.as_async().send(Message::Polled).await;
        while let Ok(msg) = async_rec.recv().await {
            // dbg!(&msg);
            if let Err(e) = match msg {
                LoaderMessage::PropertyCreate(id, container) => create_property(&wrapper, &mut ptr_h, id, container).await,
                LoaderMessage::PropertyTypeChange(id, val_container, allow_modify) => property_type_change(&wrapper, &mut ptr_h, id, val_container, allow_modify).await,
                LoaderMessage::PropertyDelete(id) => delete_property(&wrapper, &mut ptr_h, id).await,
                LoaderMessage::Shutdown => shutdown(&wrapper, &mut ptr_h),
                LoaderMessage::Subscribe(prop_handle) => subscribe_property_start(&wrapper, &mut ptr_h, prop_handle).await,
                LoaderMessage::GenerateSubscribtion(id, prop_handle) => generate_subcription(&wrapper, &mut ptr_h, id, prop_handle).await,
                LoaderMessage::UpdateSubscription(prop_handle, val_container) => update_subscription(&wrapper, &mut ptr_h, prop_handle, val_container),
                LoaderMessage::Unsubscribe(prop_handle) => unsubscribe(&wrapper, &mut ptr_h, prop_handle).await,
                LoaderMessage::HasUnsubscribed(id, prop_handle) => has_unsubscribed(&wrapper, &mut ptr_h, prop_handle, id),
                
                LoaderMessage::StartupFinished => startup_complete(&wrapper, &mut ptr_h).await,
                LoaderMessage::OtherPluginStartup(id) => send_simple_message(&wrapper, &mut ptr_h,
                    Message { sort: MessageType::OtherPluginStarted, value: MessageValue { plugin_id: id }}, "Failed on informing about other plugin"),
                LoaderMessage::InternalMessage(msg) => send_simple_message(&wrapper, &mut ptr_h,
                    Message { sort: MessageType::InternalMessage, value: MessageValue { internal_msg: msg }}, "Failed on processing plugin internal message"),
                LoaderMessage::PluginMessagePtr((origin, ptr, reason)) => send_simple_message(&wrapper, &mut ptr_h,
                    Message { sort: MessageType::PluginMessagePtr, value: MessageValue { message_ptr: MessagePtr { origin, message_ptr: ptr.ptr, reason } }},
                    "Failed to process PluginMessagePtr"),
                LoaderMessage::SendPluginMessagePtr((target, ptr, reason)) => {
                    send_plugin_message(&ptr_h, target, LoaderMessage::PluginMessagePtr((id, ptr, reason))).await.map(|okay| if !okay {
                        error!("Plugin {} failed to send message with ptr to plugin {}", get_plugin_name(&ptr_h), target);
                    })
                },

                LoaderMessage::EventTriggered(ev) => send_simple_message(&wrapper, &mut ptr_h,
                    Message { sort: MessageType::EventTriggered, value: MessageValue { event: ev } }, "Failed to pass in event trigger"),
                LoaderMessage::EventUnsubscribed(ev) => send_simple_message(&wrapper, &mut ptr_h,
                    Message { sort: MessageType::EventUnsubscribed, value: MessageValue { event: ev } }, "Failed to inform of event unsubscribe"),
                LoaderMessage::Action(action) => send_simple_message(&wrapper, &mut ptr_h,
                    Message { sort: MessageType::ActionRecv, value: MessageValue { action } }, "Failed on processing an Action"),
                LoaderMessage::ActionCallback(action) => send_simple_message(&wrapper, &mut ptr_h,
                    Message { sort: MessageType::ActionCallback, value: MessageValue { action } }, "Failed on processing the Callback for an Action"),
                

                // LoaderMessage::Update(prop_handle, value) => {
                //     let msg = LoaderMessage::Update(prop_handle, value);
                //     send_update!(wrapper, ptr_h, msg);
                //     Ok(())
                // },
                // LoaderMessage::Removed(prop_handle) => {
                //     let msg = LoaderMessage::Removed(prop_handle);
                //     send_update!(wrapper, ptr_h, msg);
                //     Ok(())
                // }
            } {
                // log out the error and exit loop
                match e {
                    MsgProcessingError::Shutdown => {
                        debug!("Plugin {} received shutdown, exiting loop", get_plugin_name(&ptr_h));
                        safe_shutdown = !ptr_h.ptr.is_null();
                    },
                    MsgProcessingError::NoneZeroReturnCode(str) => {
                        error!("Plugin {} failed, none zero return code received when executing {}", get_plugin_name(&ptr_h), str);
                    },
                    MsgProcessingError::NullPtr => {
                        error!("A Plugin could not dereference the pluginhandle due to null pointer");
                    }
                }
                break;
            }

            if async_rec.is_empty() && ptr_h.is_locked {
                // debug!("Unlock triggered");
                send_unlock(&wrapper, &mut ptr_h).unwrap();
            }
        }



        // End of life
        let name = get_plugin_name(&ptr_h);
        let mut w_store = datastore.write().await;
        if DataStoreReturnCode::Ok != w_store.delete_plugin(id, safe_shutdown).await {
            error!("Plugin {} failed to shutdown properly", name.as_str());
            drop(w_store);
            return Err(name);
        } else {
            info!("Plugin {} stopped", name);
        }
        drop(w_store);
        unsafe { ManuallyDrop::drop(&mut wrapper); }

        Ok(())
    } else {
        set_errors!(datastore);
        error!("Unable to load {} as a plugin (file could be damaged or missing necessary functions)", path.to_str().unwrap_or_default());
        Err(path.to_str().unwrap_or_default().to_string())
    }
}

// We have to do this, as you can otherwise not await anything
struct PtrWrapper {
    ptr: *mut PluginHandle,
    is_locked: bool,
    subscribers: HashMap<u64, Vec<u64>>
}

unsafe impl Send for PtrWrapper { }
unsafe impl Sync for PtrWrapper { }

fn get_plugin_name(ptr: &PtrWrapper) -> String {
    if let Some(handle) = unsafe {
        ptr.ptr.as_ref()
    } {
        return handle.name.clone();
    }

    "unknown/null pointer".to_string()
}

#[derive(WrapperApi)]
pub struct PluginWrapper {
    get_plugin_description: extern "C" fn() -> api_types::PluginDescription,
    free_string: extern "C" fn(ptr: *mut libc::c_char),
    init: extern "C" fn(handle: *mut PluginHandle) -> libc::c_int,
    update: extern "C" fn(handle: *mut PluginHandle, msg: api_types::Message) -> libc::c_int,
}

// Sketchup of what Message will internally become
#[derive(Debug)]
pub(crate) enum LoaderMessage {
    PropertyCreate(u64, utils::PropertyContainer),
    PropertyTypeChange(u64, utils::ValueContainer, bool),
    PropertyDelete(u64),
    Subscribe(PropertyHandle),
    GenerateSubscribtion(u64, PropertyHandle),
    UpdateSubscription(PropertyHandle, utils::ValueContainer),
    Unsubscribe(PropertyHandle),
    HasUnsubscribed(u64, PropertyHandle),
    
    InternalMessage(i64),
    StartupFinished,
    SendPluginMessagePtr((u64, VoidPtrWrapper, i64)),
    PluginMessagePtr((u64, VoidPtrWrapper, i64)),
    OtherPluginStartup(u64),

    EventTriggered(EventHandle),
    EventUnsubscribed(EventHandle),

    Action(Action),
    ActionCallback(Action),
    

    // Update(PropertyHandle, Value),
    // Removed(PropertyHandle),
    Shutdown

}

#[derive(Debug)]
enum MsgProcessingError {
    NoneZeroReturnCode(&'static str),
    Shutdown,
    NullPtr,

}

fn get_mut_handle<'a>(ptr: &'a PtrWrapper) -> Result<&'a mut PluginHandle, MsgProcessingError> {
    if let Some(han) = unsafe {
        ptr.ptr.as_mut()
    } {
        Ok(han)
    } else {
        Err(MsgProcessingError::NullPtr)
    }
}

fn get_handle<'a>(ptr: &'a PtrWrapper) -> Result<&'a PluginHandle, MsgProcessingError> {
    if let Some(han) = unsafe {
        ptr.ptr.as_ref()
    } {
        Ok(han)
    } else {
        Err(MsgProcessingError::NullPtr)
    }
}

async fn send_plugin_message(ptr: &PtrWrapper, id: u64, msg: LoaderMessage) -> Result<bool, MsgProcessingError> {
    let handle = get_handle(ptr)?;
    
    let ds_r = handle.datastore.read().await;

    Ok(ds_r.send_message_to_plugin(id, msg).await)
}

async fn send_message_to_all_subs<D>(ptr: &PtrWrapper, prop_id: u64, msg_factory: D) -> Result<(), MsgProcessingError>
where
    D: Fn() -> LoaderMessage
{
    let handle = get_handle(ptr)?;
    

    if let Some(subs) = ptr.subscribers.get(&prop_id) {
        let ds_r = handle.datastore.read().await;

        for su in subs {
            ds_r.send_message_to_plugin(*su, msg_factory()).await;
        }
    }

    Ok(())
}

fn send_update(wrapper: &PluginWrapper, ptr: &PtrWrapper, msg: Message, fail_error: &'static str) -> Result<(), MsgProcessingError> {
    if wrapper.update(ptr.ptr, msg) != 0 {
        return Err(MsgProcessingError::NoneZeroReturnCode(fail_error));
    }

    Ok(())
}

fn send_simple_message(wrapper: &PluginWrapper, ptr: &mut PtrWrapper, msg: Message, fail_error: &'static str) -> Result<(), MsgProcessingError> {
    send_unlock(wrapper, ptr)?;

    send_update(wrapper, ptr, msg, fail_error)
}

/// Serves to check if the handle is locked, if not change that
fn send_lock(wrapper: &PluginWrapper, ptr: &mut PtrWrapper) -> Result<(), MsgProcessingError> {
    if !ptr.is_locked {
        // We lock the plugin, then actually secure write lock
        // This is to prevent a lock trap from calls during the lock update
        send_update(wrapper, ptr, Message { sort: MessageType::Lock, value: MessageValue { flag: true } }, "Failed on lock")?;

        let han = get_handle(ptr)?;
        han.lock();
        let _ = han;
        
        ptr.is_locked = true;
    }

    Ok(())
}

fn send_unlock(wrapper: &PluginWrapper, ptr: &mut PtrWrapper) -> Result<(), MsgProcessingError> {
    if ptr.is_locked {
        let han = get_handle(ptr)?;
        han.unlock();
        let _ = han;

        ptr.is_locked = false;

        send_update(wrapper, ptr, Message { sort: MessageType::Unlock, value: MessageValue { flag: true } }, "Failed on unlock")?;
    }

    Ok(())
}

async fn create_property(wrapper: &PluginWrapper, ptr: &mut PtrWrapper, id: u64, container: utils::PropertyContainer) -> Result<(), MsgProcessingError> {
    send_lock(wrapper, ptr)?;

    let handle = get_mut_handle(ptr)?;
    
    if handle.properties.contains_key(&id) {
        // We will not create the property, instead log an error
        error!("Plugin {} failed to add property {}, id collision {}", handle.name, container.short_name, id);
        return Ok(());
    }
    let val_container = container.clone_container();
    let prop_name = format!("{}.{}", handle.name.to_lowercase(), container.short_name.to_lowercase());
    handle.properties.insert(id, container);

    // We write into datastore the property too
    let prop = PropertyHandle { plugin: handle.id, property: id };
    let mut ds_w = handle.datastore.write().await;
    ds_w.set_property(prop.clone(), val_container).await;
    ds_w.register_property_name(prop, prop_name);
    drop(ds_w);

    Ok(())
}

async fn property_type_change(wrapper: &PluginWrapper, ptr: &mut PtrWrapper, id: u64, val_container: utils::ValueContainer, allow_modify: bool) -> Result<(), MsgProcessingError> {
    send_lock(wrapper, ptr)?;
    
    let handle = get_mut_handle(ptr)?;

    if let Some(cont) = handle.properties.get_mut(&id) {
        cont.swap_container(val_container, allow_modify);

        // Technically we can unlock while sending messages, practically we have to see if there is any gain
        let prop = PropertyHandle { plugin: handle.id, property: id };
        
        let mut ds_w = handle.datastore.write().await;
        ds_w.set_property(prop, cont.clone_container()).await;
        drop(ds_w); // We could rewrite send_message to take the mutexguard... or not
        // But we have to drop it so the send can achieve lock

        send_message_to_all_subs(ptr, id, || {
            LoaderMessage::UpdateSubscription(prop.clone(), cont.clone_container())
        }).await?;
    } else {
        error!("Plugin {} failed to change type of property of id {}, it does not exist", handle.name, id);
        return Ok(());
    }

    Ok(())
}

async fn delete_property(wrapper: &PluginWrapper, ptr: &mut PtrWrapper, id: u64) -> Result<(), MsgProcessingError> {
    send_lock(wrapper, ptr)?;
    let handle = get_mut_handle(ptr)?;
    
    if !handle.properties.contains_key(&id) {
        // We will not create the property, instead log an error
        error!("Plugin {} failed to delete property of id {}, not found", handle.name, id);
        return Ok(());
    }
    handle.properties.remove(&id);

    // Technically we can unlock while sending messages, practically we have to see if there is any gain
    let prop = PropertyHandle { plugin: handle.id, property: id };
    let mut ds_w = handle.datastore.write().await;
    ds_w.delete_property(&prop).await;
    drop(ds_w); // We could rewrite send_message to take the mutexguard... or not
    // we do have to drop it, it could else never secure lock
    

    send_message_to_all_subs(ptr, id, || {
        LoaderMessage::Unsubscribe(prop.clone())
    }).await?;
    ptr.subscribers.remove(&id);

    Ok(())
}

fn shutdown(wrapper: &PluginWrapper, ptr: &mut PtrWrapper) -> Result<(), MsgProcessingError> {
    send_unlock(wrapper, ptr)?;
    
    if wrapper.update(ptr.ptr, Message { sort: MessageType::Shutdown, value: MessageValue { flag: true }}) != 0 {
        return Err(MsgProcessingError::NoneZeroReturnCode("Failed on shutdown message"));
    }

    Err(MsgProcessingError::Shutdown)
}

/// Subscribing is a 3 step process, this is done by the sub, first we send a message to the property owner
async fn subscribe_property_start(wrapper: &PluginWrapper, ptr: &mut PtrWrapper, prop_handle: PropertyHandle) -> Result<(), MsgProcessingError> {
    send_unlock(wrapper, ptr)?;

    // debug!("Entered Step 1");

    if !send_plugin_message(ptr, prop_handle.plugin, LoaderMessage::GenerateSubscribtion(get_handle(ptr)?.id, prop_handle)).await? {
        error!("Plugin {} failed to send message to generate subscription to plugin of id {} (likely plugin does not exist)", get_plugin_name(ptr), prop_handle.plugin);
        return Ok(());
    }

    Ok(())
}

/// This is Step 2, this is run by the owner, generates a shallow copy of the ValueContainer and
/// sends it back
async fn generate_subcription(wrapper: &PluginWrapper, ptr: &mut PtrWrapper, id: u64, prop_handle: PropertyHandle) -> Result<(), MsgProcessingError> {
    send_unlock(wrapper, ptr)?;

    // debug!("Entered Step 2");

    let handle = get_handle(ptr)?;
    if prop_handle.plugin != handle.id {
        error!("Plugin {} (id {}) somehow was asked for the property of plugin id {}", handle.name, handle.id, prop_handle.plugin);
        return Ok(());
    }

    let val_container = if let Some(cont) = handle.properties.get(&prop_handle.property) {
       cont.clone_container() 
    } else {
        error!("Plugin {} was requested property of id {} by plugin of id {}, but it does not exist", handle.name, prop_handle.property, id);
        return Ok(());
    };

    if !send_plugin_message(ptr, id, LoaderMessage::UpdateSubscription(prop_handle, val_container)).await? {
        error!("Plugin {} failed to send reply message to containing subscription to plugin of id {}", get_plugin_name(ptr), prop_handle.plugin);
        return Ok(());
    }

    // Adding the subscription so we can keep type changes up to date
    // We don't need to lock, due to us not writing to the pointer, the sub list is stored in the
    // wrapper, aka only this loader has access, and has anyway always mut access
    if let Some(subs) = ptr.subscribers.get_mut(&prop_handle.property) {
        subs.push(id);
    } else {
        ptr.subscribers.insert(prop_handle.property, vec![id]);
    }

    Ok(())
}

/// This is Step 3, run by the sub, we add the value container to our subscription list (for which
/// we need to lock)
/// This is also used to update the subscription, for example when the owner changed type
fn update_subscription(wrapper: &PluginWrapper, ptr: &mut PtrWrapper, prop_handle: PropertyHandle, val_container: utils::ValueContainer) -> Result<(), MsgProcessingError> {
    send_lock(wrapper, ptr)?;

    // debug!("Entered Step 3");

    let handle = get_mut_handle(ptr)?;
    // We do in this to allow overrides
    handle.subscriptions.insert(prop_handle, val_container);

    Ok(())
}

async fn unsubscribe(wrapper: &PluginWrapper, ptr: &mut PtrWrapper, prop_handle: PropertyHandle) -> Result<(), MsgProcessingError> {
    send_lock(wrapper, ptr)?;
    
    let handle = get_mut_handle(ptr)?;

    if !handle.subscriptions.contains_key(&prop_handle) {
        error!("Plugin {} failed to unsubscribe from property of plugin id {} property id {}: we weren't subscribed", get_plugin_name(ptr), prop_handle.plugin, prop_handle.property);
        return Ok(());
    }

    handle.subscriptions.remove(&prop_handle);

    if !send_plugin_message(ptr, prop_handle.plugin, LoaderMessage::HasUnsubscribed(handle.id, prop_handle)).await? {
        error!("Plugin {} failed to send reply message to containing subscription to plugin of id {}", get_plugin_name(ptr), prop_handle.plugin);
        return Ok(());
    }


    Ok(())
}

fn has_unsubscribed(wrapper: &PluginWrapper, ptr: &mut PtrWrapper, prop_handle: PropertyHandle, id: u64) -> Result<(), MsgProcessingError> {
    send_unlock(wrapper, ptr)?;

    let handle = get_handle(ptr)?;
    if prop_handle.plugin != handle.id {
        error!("Plugin {} (id {}) somehow was asked to remove subscriber from a property of plugin id {}", handle.name, handle.id, prop_handle.plugin);
        return Ok(());
    }

    if let Some(subs) = ptr.subscribers.get_mut(&prop_handle.property) {
        subs.retain(|x| *x != id);
    } else {
        // This case is not an error, and will happen due to delete_property sending unsubscribes,
        // which send this message
    }

    Ok(())
}

async fn startup_complete(wrapper: &PluginWrapper, ptr: &mut PtrWrapper) -> Result<(), MsgProcessingError> {
    send_unlock(&wrapper, ptr)?;

    let han = get_handle(ptr)?;
    let id = han.id.clone();
    let mut ds_w = han.datastore.write().await;

    ds_w.set_plugin_ready(id).await;
    
    drop(ds_w);

    send_update(&wrapper, ptr, Message { sort: MessageType::StartupFinished, value: MessageValue { flag: true } }, "Failed on informing about finshed startup")   
}
