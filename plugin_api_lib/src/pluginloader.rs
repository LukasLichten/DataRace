use std::{path::PathBuf, fs};

use dlopen2::wrapper::{WrapperApi, Container};
use hashbrown::HashMap;
use log::{error, info, debug};

use tokio::task::JoinSet;

use crate::{api_types, datastore::DataStore, utils, DataStoreReturnCode, Message, MessageType, MessageValue, PluginHandle, PropertyHandle};

pub(crate) async fn load_all_plugins(datastore: &'static tokio::sync::RwLock<DataStore>) -> Result<JoinSet<()>,Box<dyn std::error::Error>> {
    let plugin_folder = PathBuf::from("./plugins/");

    if !plugin_folder.is_dir() {
        info!("Plugins folder did not exist, creating...");
        if let Err(e) = fs::create_dir(plugin_folder.as_path()) {
            error!("Unable to create plugins folder! Exiting...");
            // TODO turn this into message into the error returned
            return Err(Box::new(e));
        }
    }

    #[cfg(target_os = "linux")]
    let ending = "so";
    #[cfg(target_os = "windows")]
    let ending = "dll";

    let mut plugin_task_handles = JoinSet::<()>::new();


    if let Ok(mut res) = fs::read_dir(plugin_folder) {
        while let Some(Ok(item)) = res.next() {
            debug!("Found {} in plugin folder", item.path().to_str().unwrap());
            if item.path().extension().unwrap().to_str().unwrap() == ending {
                plugin_task_handles.spawn(run_plugin(item.path(), datastore));
            }
        }

    }

 
    Ok(plugin_task_handles)
}

async fn run_plugin(path: PathBuf, datastore: &'static tokio::sync::RwLock<DataStore>) {
    if let Ok(wrapper) = unsafe { Container::<PluginWrapper>::load(path.to_str().unwrap()) } {
        // Preperations
        let desc = wrapper.get_plugin_description();

        let name = if let Some(n) = utils::get_string(desc.name) {
            wrapper.free_string(desc.name);
            n
        } else {
            error!("Unable to parse plugin name, id {}", desc.id);
            wrapper.free_string(desc.name);
            return;
        };

        if desc.api_version == u64::MAX {
            // Missmatched API Version
            error!("API version must be set at compiletime of your plugin ({}), requesting a API Version from the library during runtime will only return u64::MAX (DataRace is running on api version {})",
                name.as_str(), crate::API_VERSION);
            return;
        }
        if desc.api_version != crate::API_VERSION {
            // Missmatched API Version
            error!("Missmatched api version for plugin {}, will not be launched: Build for api {} (DataRace is running on {})", name.as_str(), desc.api_version, crate::API_VERSION);
            return;
        }

        // Verifying ID is generated correctly with hash
        let id = if let Some(id) = utils::generate_plugin_name_hash(name.as_str()) {
            if id != desc.id {
                error!("Plugin id set by plugin {} does not match the id generated by the name: Given {}, Expected: {}", name.as_str(), desc.id, id);
                return;
            }
            id
        } else {
            error!("Unable to verify plugin id set by plugin {}: plugin name does not comply with naming schema", name.as_str());
            return;
        };

        // Creates PluginHandle
        let (sender, receiver) = utils::get_message_channel();
        let handle = PluginHandle::new(name, id, datastore, sender.clone(), wrapper.free_string.clone(), desc.version);
        let mut ptr_h = PtrWrapper { ptr: Box::into_raw(Box::new(handle)), is_locked: false, subscribers: HashMap::default() };

        drop(desc); // drop is importantent, name ptr is pointing at freed memory

        let mut w_store = datastore.write().await;
        if w_store.register_plugin(id, sender.clone(), ptr_h.ptr).is_none() {
            if w_store.get_shutdown_status() {
                error!("Unable to register Plugin {}, shut down already in progress", get_plugin_name(&ptr_h));
                return;
            }

            error!("Unable to register Plugin {} (id {}), name/id collision", get_plugin_name(&ptr_h), id);
            return;
        }
        drop(w_store);

        debug!("Plugin {} with id {} loaded", get_plugin_name(&ptr_h), id);

        // Initializing
        if wrapper.init(ptr_h.ptr) != 0 {
            // None Zero Error Code, shut down
            error!("Plugin {} failed to initialize", get_plugin_name(&ptr_h));
            // Remove Plugin again from pluginstore
            return;
        }

        let async_rec = receiver.to_async();

        // let _ = sender.as_async().send(Message::Polled).await;
        while let Ok(msg) = async_rec.recv().await {
            // dbg!(&msg);
            if let Err(e) = match msg {
                LoaderMessage::PropertyCreate(id, container) => create_property(&wrapper, &mut ptr_h, id, container),
                LoaderMessage::PropertyTypeChange(id, val_container, allow_modify) => property_type_change(&wrapper, &mut ptr_h, id, val_container, allow_modify).await,
                LoaderMessage::PropertyDelete(id) => delete_property(&wrapper, &mut ptr_h, id).await,
                LoaderMessage::Shutdown => shutdown(&wrapper, &mut ptr_h),
                LoaderMessage::Subscribe(prop_handle) => subscribe_property_start(&wrapper, &mut ptr_h, prop_handle).await,
                LoaderMessage::GenerateSubscribtion(id, prop_handle) => generate_subcription(&wrapper, &mut ptr_h, id, prop_handle).await,
                LoaderMessage::UpdateSubscription(prop_handle, val_container) => update_subscription(&wrapper, &mut ptr_h, prop_handle, val_container),
                LoaderMessage::Unsubscribe(prop_handle) => unsubscribe(&wrapper, &mut ptr_h, prop_handle).await,
                LoaderMessage::HasUnsubscribed(id, prop_handle) => has_unsubscribed(&wrapper, &mut ptr_h, prop_handle, id),
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
                    },
                    MsgProcessingError::NoneZeroReturnCode(str) => {
                        error!("Plugin {} failed, none zero return code received when executing {}", get_plugin_name(&ptr_h), str);
                    },
                    MsgProcessingError::NullPtr => {
                        error!("A Plugin could not dereference the pluginhandle due to null pointer");
                        return;
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
        if DataStoreReturnCode::Ok != w_store.delete_plugin(id).await {
            error!("Plugin {} failed to shutdown properly", name);
        } else {
            info!("Plugin {} stopped", name);
        }
        drop(w_store);
        
    } else {
        debug!("Unable to load {} as a plugin", path.to_str().unwrap());
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

    "unknown".to_string()
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

/// Serves to check if the handle is locked, if not change that
fn send_lock(wrapper: &PluginWrapper, ptr: &mut PtrWrapper) -> Result<(), MsgProcessingError> {
    if !ptr.is_locked {
        // We lock the plugin, then actually secure write lock
        // This is to prevent a lock trap from calls during the lock update
        if wrapper.update(ptr.ptr, Message { sort: MessageType::Lock, value: MessageValue { flag: true } }) != 0 {
            return Err(MsgProcessingError::NoneZeroReturnCode("Failed on lock"))
        }

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

        if wrapper.update(ptr.ptr, Message { sort: MessageType::Unlock, value: MessageValue { flag: true } }) != 0 {
            return Err(MsgProcessingError::NoneZeroReturnCode("Failed on unlock"))
        }
    }

    Ok(())
}

fn create_property(wrapper: &PluginWrapper, ptr: &mut PtrWrapper, id: u64, container: utils::PropertyContainer) -> Result<(), MsgProcessingError> {
    send_lock(wrapper, ptr)?;

    let handle = get_mut_handle(ptr)?;
    
    if handle.properties.contains_key(&id) {
        // We will not create the property, instead log an error
        error!("Plugin {} failed to add property {}, id collision {}", handle.name, container.short_name, id);
        return Ok(());
    }
    handle.properties.insert(id, container);

    Ok(())
}

async fn property_type_change(wrapper: &PluginWrapper, ptr: &mut PtrWrapper, id: u64, val_container: utils::ValueContainer, allow_modify: bool) -> Result<(), MsgProcessingError> {
    send_lock(wrapper, ptr)?;
    
    let handle = get_mut_handle(ptr)?;

    if let Some(cont) = handle.properties.get_mut(&id) {
        cont.swap_container(val_container, allow_modify);

        // Technically we can unlock while sending messages, practically we have to see if there is
        // any gain
        send_message_to_all_subs(ptr, id, || {
            LoaderMessage::UpdateSubscription(PropertyHandle { plugin: handle.id, property: id }, cont.clone_container())
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

    // Technically we can unlock while sending messages, practically we have to see if there is
    // any gain
    send_message_to_all_subs(ptr, id, || {
        LoaderMessage::Unsubscribe(PropertyHandle { plugin: handle.id, property: id })
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
