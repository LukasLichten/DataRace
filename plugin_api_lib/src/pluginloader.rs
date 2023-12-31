use std::{path::PathBuf, fs};

use dlopen2::wrapper::{WrapperApi, Container, WrapperMultiApi};
use kanal::Sender;
use log::{error, info, debug};

use tokio::task::JoinSet;

use crate::{PluginHandle, utils::{self, Value}, datastore::DataStore, DataStoreReturnCode, PropertyHandle};

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
        let name = if let Some(ref name_handle) = wrapper.name {
            let ptr = name_handle.get_plugin_name();
            let n = utils::get_string(ptr).clone();
            
            name_handle.free_plugin_name(ptr);

            n
        } else {
            None
        };

        let name = if let Some(name) = name {
            name
        } else {
            path.file_stem().unwrap().to_str().unwrap().to_string()
        };


        let mut w_store = datastore.write().await;
        let (token, reciever, sender) = if let Some(v) = w_store.create_plugin(name.clone()) {
            v
        } else {
            error!("Unable to register Plugin {}, name collision", name);
            return;
        };
        drop(w_store);

        let handle = PluginHandle { name, datastore, token: token.clone() };
        let ptr_h = PtrWrapper { ptr: Box::into_raw(Box::new(handle)) };

        // Initializing
        if wrapper.func.init(ptr_h.ptr) != 0 {
            // None Zero Error Code, shut down
            error!("Plugin {} failed to initialize", get_plugin_name(&ptr_h));
            return;
        }


        // Main Run loop
        let mut poller = tokio::spawn(store_list(vec![], vec![]));

        let _temp = sender.as_async().send(Message {}).await;
        while let Ok(_msg) = reciever.as_async().recv().await {
            let (list, changed) = poller.await.unwrap();
            poller = tokio::spawn(poll_propertys(datastore, list, changed, sender));

            // Currently we can't do anything
            break;
        }



        // End of life
        let mut w_store = datastore.write().await;
        if DataStoreReturnCode::Ok != w_store.delete_plugin(&token).await {
            error!("Plugin {} failed to shutdown properly", get_plugin_name(&ptr_h));
        } else {
            info!("Plugin {} stopped", get_plugin_name(&ptr_h));
        }
        drop(w_store);


        // Should we deallocate the PluginHandle?
        // Well, the Plugin should have stopped any threads it has, all calls it does have ceased,
        // so we should be in the clear
        unsafe {
            drop(Box::from_raw(ptr_h.ptr));
        }

        
    } else {
        debug!("Unable to load {} as a plugin", path.to_str().unwrap());
    }
}

async fn poll_propertys(
    datastore: &'static tokio::sync::RwLock<DataStore>,
    mut list: Vec<(PropertyHandle, Value)>,
    mut changed: Vec<(PropertyHandle, Value)>,
    sender: Sender<Message>
) -> (Vec<(PropertyHandle, Value)>,Vec<(PropertyHandle, Value)>) {
    let ds = datastore.read().await;

    changed.clear();
    
    let mut index = 0;
    while let Some((handle, last_value)) = list.get_mut(index) {
        match ds.get_property(handle).await {
            Ok(res) => {
                if res != *last_value {
                    // Value changed
                    changed.push((handle.clone(), res.clone()));
                    *last_value = res;
                }
            },
            Err(DataStoreReturnCode::OutdatedPropertyHandle) => {
                // we need to send a message that this property got lost
                let _ = sender.as_async().send(Message {}).await;

                list.remove(index); // We will remove this item

                // We have to re-iterate over the item on this index,
                // as all items got moved forward
                index -= 1; 
            },
            Err(_) => () // this should not happen
        }
        index += 1;
    }

    let _ = sender.as_async().send(Message {}).await;

    (list,changed)
}

/// Serves to temporarily store the list within a joinhandle
/// this is used when no subscriptions exist, or when subscribing to a new property
async fn store_list(list: Vec<(PropertyHandle, Value)>, changed: Vec<(PropertyHandle, Value)>) -> (Vec<(PropertyHandle, Value)>, Vec<(PropertyHandle, Value)>) {
    (list,changed)
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

#[derive(WrapperMultiApi)]
pub struct PluginWrapper {
    name: Option<PluginNameWrapper>,
    func: PluginFuncWrapper
}

#[derive(WrapperApi)]
struct PluginNameWrapper {
    get_plugin_name: extern "C" fn() -> *mut libc::c_char,
    free_plugin_name: extern "C" fn(ptr: *mut libc::c_char),
}

#[derive(WrapperApi)]
struct PluginFuncWrapper {
    init: extern "C" fn(handle: *mut PluginHandle) -> libc::c_int
}

pub(crate) struct Message {

}

// Sketchup of what Message will internally become
enum Mess {
    Polled,
    Subscribe(PropertyHandle),
    Unsubscribe(PropertyHandle),
    Update((PropertyHandle, Value)),
    Removed(PropertyHandle),
    Shutdown

}



