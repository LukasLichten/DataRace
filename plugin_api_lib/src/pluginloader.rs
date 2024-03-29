use std::{path::PathBuf, fs};

use dlopen2::wrapper::{WrapperApi, Container, WrapperMultiApi};
use kanal::Sender;
use log::{error, info, debug};

use tokio::task::JoinSet;

use crate::{api_types, datastore::DataStore, utils::{self, Value}, DataStoreReturnCode, PluginHandle, PropertyHandle};

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

macro_rules! send_update {
    ($wrapper:ident, $ptr_h:ident, $msg: ident) => {
        if let Ok(msg) = api_types::Message::try_from($msg) {
            if $wrapper.update($ptr_h.ptr, msg) != 0 {
                error!("Plugin {} failed on update", get_plugin_name(&$ptr_h));
                break;
            }     
        } else {
            // What do we do if the parse is not okay? idk...
            error!("Failed to parse message ");
        }
    };
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

        if desc.api_version != crate::API_VERSION {
            // Missmatched API Version
            error!("Missmatched api version for plugin {}, will not be launched: Build for api {} (DataRace is running on {})", name.as_str(), desc.api_version, crate::API_VERSION);
            return;
        }

        // Verifying ID is generated correctly with hash
        // TODO
        let id = desc.id;

        // TODO use version number

        drop(desc); // drop is importantent, name ptr is pointing at freed memory
        

        // Creates PluginHandle
        let handle = PluginHandle::new(name, id, datastore);
        let ptr_h = PtrWrapper { ptr: Box::into_raw(Box::new(handle)) };
        let (sender, receiver) = utils::get_message_channel();

        let mut w_store = datastore.write().await;
        if w_store.register_plugin(id, sender.clone(), ptr_h.ptr).is_none() {
            if w_store.get_shutdown_status() {
                error!("Unable to register Plugin {}, shut down already in progress", get_plugin_name(&ptr_h));
                return;
            }

            error!("Unable to register Plugin {}, name collision", get_plugin_name(&ptr_h));
            return;
        }
        drop(w_store);

        // Initializing
        if wrapper.init(ptr_h.ptr) != 0 {
            // None Zero Error Code, shut down
            error!("Plugin {} failed to initialize", get_plugin_name(&ptr_h));
            return;
        }

        let async_rec = receiver.to_async();

        // let _ = sender.as_async().send(Message::Polled).await;
        while let Ok(msg) = async_rec.recv().await {
            match msg {
                Message::Shutdown => break,
                Message::Subscribe(prop_handle) => {
                    // // We will finish the polling so we add this prop handle to the list, then let
                    // // the list be stored dormant in a task till needed
                    // let (mut list, changed) = poller.await.unwrap();
                    //
                    // if list.len() == 0 {
                    //     // To start the polling task proper
                    //     let _ = sender.as_async().send(Message::Polled).await;
                    // }
                    //
                    // // Making sure we don't subscribe to it twice
                    // let mut is_present = false;
                    // for (old_prop_handle, _) in list.iter() {
                    //     if old_prop_handle == &prop_handle {
                    //         is_present = true;
                    //         break;
                    //     }
                    // }
                    // 
                    // if !is_present {
                    //     list.push((prop_handle, Value::None));
                    // }
                    // 
                    // poller = tokio::spawn(store_list(list, changed))
                },
                Message::Unsubscribe(prop_handle) => {
                    // let (mut list, changed) = poller.await.unwrap();
                    //
                    // let mut index = 0;
                    // while let Some((han, _)) = list.get(index) {
                    //     if han.index == prop_handle.index && han.hash == prop_handle.hash {
                    //         break;
                    //     }
                    //     index += 1;
                    // }
                    //
                    // if index < list.len() {
                    //     list.remove(index);
                    // }
                    //
                    // poller = tokio::spawn(store_list(list, changed))
                },
                Message::Polled => {
                    // let (list, changed) = poller.await.unwrap();
                    // 
                    // for (prop_handle, value) in changed {
                    //     let msg = Message::Update(prop_handle, value);
                    //     send_update!(wrapper, ptr_h, msg);
                    // }
                    //
                    // // changed.clear();
                    // let changed = vec![];
                    // poller = if list.len() == 0 {
                    //     tokio::spawn(store_list(list, changed))
                    // } else {
                    //     tokio::spawn(poll_propertys(datastore, list, changed, sender.clone()))
                    // };
                },
                Message::Update(prop_handle, value) => {
                    let msg = Message::Update(prop_handle, value);
                    send_update!(wrapper, ptr_h, msg);
                },
                Message::Removed(prop_handle) => {
                    let msg = Message::Removed(prop_handle);
                    send_update!(wrapper, ptr_h, msg);
                }
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
    ptr: *mut PluginHandle
}

unsafe impl Send for PtrWrapper { }

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
pub(crate) enum Message {
    Polled,
    Subscribe(PropertyHandle),
    Unsubscribe(PropertyHandle),
    Update(PropertyHandle, Value),
    Removed(PropertyHandle),
    Shutdown

}



