use hashbrown::HashMap;
use tokio::time::{self, Duration, Instant};
use kanal::AsyncReceiver;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use socketioxide::{extract::{Data, SocketRef, State}, SocketIo};

use crate::{utils::{Value, ValueCache}, PropertyHandle};

use super::utils::{DataStoreLocked, SocketChMsg, SocketDataRef};

pub(super) async fn create_socketio_layer(datastore: DataStoreLocked) -> socketioxide::layer::SocketIoLayer {
    let (store,rx) = super::utils::SocketData::new(datastore);

    let (layer, io) = SocketIo::builder()
        .with_state(store)
        .build_layer();

    io.ns("/", on_connect);

    tokio::task::spawn(update(io, store, rx));

    layer
}

async fn on_connect(socket: SocketRef) {
    debug!("Someone is trying to connect, {}", socket.id);

    // For some reason I can't serialize the Plugin version through Serializer,
    // the function just isn't called
    socket.on("auth-dashboard", |socket: SocketRef, Data(name): Data<String>, State(store): State<SocketDataRef>| async move {
        debug!("{} socket trying to auth as dashboard {}", socket.id, &name);

        if store.get_auth(&socket.id).await.is_some() {
            // This is an error, you should not be able to auth twice
            error!("Already Authericed");
            return;
        }

        store.insert_dashboard(socket.id, name.clone()).await;
        let _ = socket.join(format!("dash.{}", name));
    });

    // socket.on("message", |socket: SocketRef, Data(data): Data<serde_json::Value>, State(store): State<SocketDataRef>| async move {
    //     let name = match store.get_auth(&socket.id).await {
    //         Some(Auth::Consumer) => "Consumer".to_string(),
    //         Some(Auth::Plugin(_, name)) => format!("Plugin {}", name),
    //         None => "Un-Authericed".to_string()
    //     };
    //     socket.emit("message-back", format!("Hello, World! {}", name)).ok();
    //     
    // });
    //
    //
    //
    // socket.on("get_property", |socket: SocketRef, Data(_data): Data<serde_json::Value>, State(store): State<SocketDataRef>| async move {
    //     // let r_store = store.datastore.read().await;
    //     // let han = r_store.get_property_handle("sample_plugin.Test".to_string()).unwrap();
    //     // let val = r_store.get_property(&han).await.unwrap();
    //     //
    //     // drop(r_store);
    //     //
    //     // socket.emit("get_property-return", if let crate::utils::Value::Int(i) = val { i.to_string() } else { "null".to_string() } ).ok();
    //     
    // });

    socket.on_disconnect(|socket: SocketRef, State(store): State<SocketDataRef>| async move {
        store.remove_auth(&socket.id).await;

        debug!("Left *big sad*");
    });
    
    let _ = socket.emit("require-auth", ());
}

const UPDATE_RATE: Duration = Duration::from_millis(10);

type UpdatePackage = Vec<(PropertyHandle, Value)>;

async fn update(io: SocketIo, datastore: SocketDataRef, rx: AsyncReceiver<SocketChMsg>) {
    let mut props = HashMap::<PropertyHandle, (ValueCache, Vec<String>)>::new();
    let mut cache = HashMap::<String, (UpdatePackage, usize)>::new();

    loop {
        // Timing start
        let update_cycle_end_time = Instant::now() + UPDATE_RATE;

        // Code start, aquiring messages
        if let Ok(Some(msg)) = rx.try_recv() {
            process_msg(msg, datastore, &mut props, &mut cache).await;
        }

        // Updating
        let ds_r = datastore.datastore.read().await;
        for (handle, (value_cache, dashes)) in props.iter_mut() {
            let new = if let Some(cont) = ds_r.get_property_container(handle) {
                cont.read_web(value_cache)
            } else {
                if value_cache.value != Value::None {
                    value_cache.value = Value::None;
                    true
                } else {
                    false
                }
            };
            
            if new {
                for d in dashes {
                    if let Some((list, _)) = cache.get_mut(d) {
                        list.push((handle.clone(), value_cache.value.clone()));
                    }
                }
            }
        }
        drop(ds_r);

        // Sending
        for (name, (list, _)) in cache.iter_mut() {
            if !list.is_empty() {
                if let Err(e) = io.within(format!("dash.{}", name)).emit("update", [&list]) {
                    error!("Failed to send update to dashboard {}: {}", name, e);
                } else {
                    list.clear();
                }
            }

            // let _ = io.within(format!("dash.{}", name)).emit("test", format!("FreeBird!")).ok();
        }

        // Sleeping to keep the update rate
        time::sleep_until(update_cycle_end_time).await;
    }
}

async fn process_msg(
    msg: SocketChMsg,
    datastore: SocketDataRef,
    props: &mut HashMap<PropertyHandle, (ValueCache, Vec<String>)>,
    cache: &mut HashMap<String, (UpdatePackage, usize)>
) {
    // debug!("Socket updater received message");
    match msg {
        SocketChMsg::AddDashboard(name) => {
            if let Ok(dash) = super::get_dashboard(datastore.datastore, name.clone()).await {
                let list = dash.list_properties();

                for p in list {
                    if let Some((value_cache, dashes)) = props.get_mut(&p) {
                        *value_cache = ValueCache::default(); // Forces a refresh
                        
                        if !dashes.contains(&name) {
                            // Maybe another instance of this dashboard already subscribed to it
                            dashes.push(name.clone());
                        }
                    } else {
                        props.insert(p, (ValueCache::default(), vec![name.clone()]));
                    }
                }
                
                if let Some((_, count)) = cache.get_mut(&name) {
                    *count += 1;
                } else {
                    cache.insert(name, (UpdatePackage::new(), 1));
                }
            } else {
                error!("Dashboard {} tried to connect to websocket, but was unable to load file to start update (Did you delete the Dashboard?)", name);
            }
        },
        SocketChMsg::RmDashboard(name) => {
            if let Some((_, count)) = cache.get_mut(&name) {
                *count -= 1;
                
                // If there are no more instances of this dashboard we remove it and it's properties
                // This may take a moment
                if *count == 0 {
                    debug!("Last Dashboard {} was removed, cleaning up...", &name);
                    let mut removal = Vec::<PropertyHandle>::new();

                    // Removing dash from the update list of every property
                    for (handle, (_, dashes)) in props.iter_mut() {
                        dashes.retain(|d| d != &name);

                        if dashes.is_empty() {
                            removal.push(handle.clone());
                        }
                    }

                    // Deleting all properties without any Dashboard
                    for item in removal {
                        props.remove(&item);
                    }
                    
                }
            }
            
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Authentication {
    Dashboard{name: String},
    Plugin{name: String} // this should serialize to {"Plugin": {"name": "test"}}
}
