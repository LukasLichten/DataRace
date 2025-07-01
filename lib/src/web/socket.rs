use std::{collections::HashMap, net::IpAddr};

use axum::RequestPartsExt;
use tokio::time::Duration;
use log::{debug, error, trace};
use serde::{Deserialize, Serialize};
use socketioxide::{extract::{Data, SocketRef, State}, SocketIo};
use datarace_socket_spec::socket::{Action, DashboardAuth, UpdatePackage, Value};

use crate::{utils::{ValueCache, ValueContainer}, PropertyHandle};

use super::{utils::{DataStoreLocked, SocketChMsg, SocketDataRef}, WebSocketChReceiver};

pub(super) async fn create_socketio_layer(datastore: DataStoreLocked, websocket_ch_recv: WebSocketChReceiver) -> socketioxide::layer::SocketIoLayer {
    let store = super::utils::SocketData::new(datastore).await;

    // We empty the queue, as likely a bunch of property creations have flooded the queue 
    while !websocket_ch_recv.is_empty() {
        let _ = websocket_ch_recv.recv().await;
    }

    let (layer, io) = SocketIo::builder()
        .with_state(store)
        .build_layer();

    io.ns("/", on_connect);

    tokio::task::spawn(update(io, store, websocket_ch_recv));

    layer
}

async fn on_connect(socket: SocketRef) {
    fn error_log(send_result: Result<(), socketioxide::SendError>) {
        if let Err(e) = send_result {
            error!("Emitting Event failed: {e}");
        }
    }

    let ip = extract_ip(&socket).await;

    debug!("Someone is trying to connect to the socket, {}:{}", ip.map(|i| i.to_string()).unwrap_or_default(), socket.id);

    // For some reason I can't serialize the Plugin version through Serializer,
    // the function just isn't called
    socket.on("auth_dashboard", |socket: SocketRef, Data(auth): Data<DashboardAuth>, State(store): State<SocketDataRef>| async move {
        debug!("{} socket trying to auth as dashboard {}", socket.id, &auth.name);

        if store.get_auth(&socket.id).await.is_some() {
            // This is an error, you should not be able to auth twice
            error!("Already Authenticated");
            return;
        }

        if store.insert_dashboard(socket.id, auth.name.clone(), auth.token.clone()).await {
            trace!("Authentication succeeded");
            socket.join(format!("dash.{}", auth.name));
        } else {
            debug!("Dashboard auth failed, emitting reload");
            trace!("Failed auth token was: {}", auth.token);
            error_log(socket.emit("require_reload", &()));
        }
    });

    socket.on("trigger_action", |socket: SocketRef, Data(action): Data<Action>, State(store): State<SocketDataRef>| async move {
        // debug!("Action trigger called");
        match store.trigger_action(&socket.id, action).await {
            Ok(id) => trace!("Action triggerd, id {}", id),
            Err(e) => error!("Failed to trigger Action: {}", e)
        }

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
    
    error_log(socket.emit("require_auth", &()));
}

async fn extract_ip(socket: &SocketRef) -> Option<IpAddr> {
    let res = socket.req_parts().clone().extract::<axum_client_ip::ClientIp>().await;

    match res {
        Ok(axum_client_ip::ClientIp(ip)) => {
            trace!("Socket IP: {ip}");
            Some(ip)
        },
        Err(e) => {
            trace!("Failed to extract ip: {e}");
            None
        }
    }
}

const UPDATE_RATE: Duration = Duration::from_millis(10);

async fn update(io: SocketIo, datastore: SocketDataRef, rx: WebSocketChReceiver) {
    let mut props = HashMap::<PropertyHandle, (ValueContainer, ValueCache, Vec<String>)>::new();
    let mut cache = HashMap::<String, (UpdatePackage, usize)>::new();

    let mut interval = tokio::time::interval(UPDATE_RATE);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);

    loop {
        // Code start, aquiring messages
        while let Ok(Some(msg)) = rx.try_recv() {
            let do_more = process_msg(msg, datastore, &mut props, &mut cache).await;
            if !do_more {
                break;
            }
        }

        // Updating
        for (handle, (container, value_cache, dashes)) in props.iter_mut() {
            let new_data = container.read_web(value_cache);
            
            if new_data {
                let val = if let Some(arr) = &value_cache.change {
                    Value::ArrUpdate(arr.clone())
                } else {
                    value_cache.value.clone()
                };

                for d in dashes {
                    if let Some((list, _)) = cache.get_mut(d) {
                        list.push((handle.clone().into(), val.clone()));
                    }
                }
            }
        }

        // Sending
        for (name, (list, _)) in cache.iter_mut() {
            if !list.is_empty() {
                if let Err(e) = io.within(format!("dash.{}", name)).emit("update", &[&list]).await {
                    error!("Failed to send update to dashboard {}: {}", name, e);
                } else {
                    list.clear();
                }
            }

            // let _ = io.within(format!("dash.{}", name)).emit("test", format!("FreeBird!")).ok();
        }

        // Sleeping to keep the update rate
        interval.tick().await;
    }
}

async fn process_msg(
    msg: SocketChMsg,
    datastore: SocketDataRef,
    props: &mut HashMap<PropertyHandle, (ValueContainer, ValueCache, Vec<String>)>,
    cache: &mut HashMap<String, (UpdatePackage, usize)>,
) -> bool {
    // debug!("Socket updater received message");
    match msg {
        SocketChMsg::AddDashboard(name) => {
            if let Ok(dash) = super::get_dashboard(datastore.datastore, name.clone()).await {
                let list = dash.list_properties();

                for p in list {
                    if let Some(prop_handle) = PropertyHandle::new(p.as_str()) {
                        if let Some((_, value_cache, dashes)) = props.get_mut(&prop_handle) {
                            *value_cache = ValueCache::default(); // Forces a refresh
                            
                            if !dashes.contains(&name) {
                                // Maybe another instance of this dashboard already subscribed to it
                                dashes.push(name.clone());
                            }
                        } else {
                            let ds_r = datastore.datastore.read().await;

                            let cont = if let Some(cont) = ds_r.get_property_container(&prop_handle) {
                                cont.shallow_clone()
                            } else {
                                ValueContainer::None
                            };
                            drop(ds_r);

                            props.insert(prop_handle, (cont, ValueCache::default(), vec![name.clone()]));
                        }
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

            false
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
                    for (handle, (_, _, dashes)) in props.iter_mut() {
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

            false
        },
        SocketChMsg::ChangedProperty(handle, container) => {
            if let Some((cont, _, _)) = props.get_mut(&handle) {
                *cont = container;
            }

            true
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Authentication {
    Dashboard{name: String},
    Plugin{name: String} // this should serialize to {"Plugin": {"name": "test"}}
}
