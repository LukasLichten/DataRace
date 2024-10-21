use hashbrown::HashMap;
use kanal::{AsyncReceiver, AsyncSender, Sender};
use log::{debug, error};
use tokio::task::JoinHandle;

use crate::{pluginloader::LoaderMessage, EventHandle};

pub(crate) fn create_event_task() -> (JoinHandle<()>, Sender<EventMessage>) {
    let (s, r) = kanal::unbounded();

    (tokio::spawn(event_loop(r.to_async(), s.clone().to_async())),s)
}

async fn event_loop(recv: AsyncReceiver<EventMessage>, sender: AsyncSender<EventMessage>) {
    debug!("Starting EventHandler Loop");

    // The boolean serves to declare if the event has been created, or if there are only
    // subscribers waiting for creation
    let mut mappings = HashMap::<EventHandle, (bool, HashMap<u64, AsyncSender<LoaderMessage>>)>::new();

    while let Ok(msg) = recv.recv().await {
        match msg {
            EventMessage::Shutdown => { break; },
            EventMessage::Trigger(ev) => {
                if let Some((_,listeners)) = mappings.get(&ev) {
                    for (plugin, sender) in listeners.iter() {
                        if let Err(e) = sender.send(LoaderMessage::EventTriggered(ev)).await {
                            error!("Unable to inform plugin {plugin} of the event {}|{} triggering: {e}", ev.plugin, ev.event);
                        }
                    }
                }
            },
            EventMessage::Create(ev) => {
                if let Some((created,_)) = mappings.get_mut(&ev) {
                    *created = true;
                } else {
                    mappings.insert(ev, (true, HashMap::new()));
                }
            },
            EventMessage::Remove(ev) => {
                if let Some((_,listeners)) = mappings.remove(&ev) {
                    for (plugin, sender) in listeners.iter() {
                        if let Err(e) = sender.send(LoaderMessage::EventUnsubscribed(ev)).await {
                            error!("Unable to inform plugin {plugin} of event {}|{} being deleted: {e}", ev.plugin, ev.event);
                        }
                    }
                }
            },
            EventMessage::Subscribe(ev, plugin, channel) => {
                if let Some((_, listeners)) = mappings.get_mut(&ev) {
                    listeners.insert(plugin, channel);
                } else {
                    // If the event already exists we allow pre subscribing
                    let mut listeners = HashMap::new();
                    listeners.insert(plugin, channel);
                    mappings.insert(ev, (false, listeners));
                }
            },
            EventMessage::Unsubscribe(ev, plugin) => {
                if let Some((_, listeners)) = mappings.get_mut(&ev) {
                    if let Some(channel) = listeners.remove(&plugin) {
                        if let Err(e) = channel.send(LoaderMessage::EventUnsubscribed(ev)).await {
                            error!("Unable to inform plugin {plugin} of event {}|{} was unsubscribed: {e}", ev.plugin, ev.event);
                        }
                    }
                }
            },
            EventMessage::RemovePlugin(plugin) => {
                for (ev, (_, listeners)) in mappings.iter_mut() {
                    if ev.plugin == plugin {
                        let _ = sender.send(EventMessage::Remove(ev.clone())).await;
                    } else {
                        let _ = listeners.remove(&plugin);
                    }
                }
            }
        }
    }
    
    debug!("EventHandler shutdown");
}

#[derive(Debug)]
pub(crate) enum EventMessage {
    Create(EventHandle),
    Remove(EventHandle),
    Subscribe(EventHandle, u64, AsyncSender<LoaderMessage>),
    Unsubscribe(EventHandle, u64),

    Trigger(EventHandle),

    Shutdown,
    RemovePlugin(u64)
}
