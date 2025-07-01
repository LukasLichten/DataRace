use std::{collections::HashMap, net::{IpAddr, Ipv4Addr, Ipv6Addr}, sync::Arc};

use hex::FromHex;
use kanal::{AsyncReceiver, AsyncSender};
use socketioxide::socket::Sid;
use tokio::sync::RwLock;
use crate::{datastore::DataStore, utils::U256, ActionHandle, PropertyHandle};
use datarace_socket_spec::socket::{Action, PropertyHandle as WebPropertyHandle, ActionHandle as WebActionHandle};

use super::dashboard::WebDashboard;

pub(super) type DataStoreLocked = &'static RwLock<DataStore>;
pub(super) type SocketDataRef = &'static SocketData;
pub(crate) type WebSocketChReceiver = AsyncReceiver<SocketChMsg>;

#[derive(Debug, Clone)]
pub(super) enum Auth {
    Dashboard(String, Vec<ActionHandle>),
    #[allow(dead_code)]
    Plugin(u64, Arc<String>)
} 

pub(crate) fn create_websocket_channel() -> (AsyncSender<SocketChMsg>, WebSocketChReceiver) {
    kanal::unbounded_async()
}

pub(super) struct SocketData {
    pub datastore: DataStoreLocked,
    access_table: RwLock<HashMap<Sid, Auth>>,
    pub sender: AsyncSender<SocketChMsg>
}

impl SocketData {
    pub(super) async fn new(datastore: DataStoreLocked) -> SocketDataRef {
        let ds_r = datastore.read().await;
        let sx = ds_r.get_websocket_channel().clone();
        drop(ds_r);

        Box::leak(
            Box::new(
                SocketData {
                    datastore,
                    access_table: RwLock::new(HashMap::new()),
                    sender: sx
                }
            )
        )
    }

    pub(super) async fn insert_auth(&self, id: Sid, auth: Auth) {
        let mut w_table = self.access_table.write().await;
        w_table.insert(id, auth);
        drop(w_table);
    }

    pub(super) async fn insert_dashboard(&self, id: Sid, name: String, token: String) -> bool {
        if let Ok(dash) = super::get_dashboard(self.datastore, name.clone()).await {

            if let Ok(token) = U256::from_hex(token) {
                let ds_r = self.datastore.read().await;
                let secret = ds_r.dashboard_hasher_secret.clone();
                drop(ds_r);

                let reference_token = dash.generate_token(secret);
                if token != reference_token {
                    return false;
                }
            } else {
                return false;
            }


            // Generates the allow_list of actions this dashboard is allowed to perform
            let set = dash.list_actions();
            let mut allow_list = Vec::<ActionHandle>::with_capacity(set.len());
            for item in set {
                if let Some(handle) = ActionHandle::new(item.as_str()) {
                    allow_list.push(handle);
                }
            }

            
            self.insert_auth(id, Auth::Dashboard(name.clone(), allow_list)).await;

            let _ = self.sender.send(SocketChMsg::AddDashboard(name)).await;
            return true;
        }

        false
    }

    pub(super) async fn get_auth(&self, id: &Sid) -> Option<Auth> {
        let r_table = self.access_table.read().await;
        if let Some(value) = r_table.get(id) {
            let val = value.clone();
            drop(r_table);

            Some(val)
        } else {
            drop(r_table);
            None
        }
    }

    pub(super) async fn remove_auth(&self, id: &Sid) {
        let mut w_table = self.access_table.write().await;
        if let Some(res) = w_table.remove(id) {
            drop(w_table);
            match res {
                Auth::Dashboard(name, _) => { 
                    let _ = self.sender.send(SocketChMsg::RmDashboard(name)).await;
                },
                Auth::Plugin(_, _) => todo!("Plugin removal not yet implemented")
            }
        }
    }

    pub(super) async fn trigger_action(&self, id: &Sid, action: Action) -> Result<u64, String> {
        let origin = match self.get_auth(id).await.ok_or(format!("Client {id} has not done auth yet"))? {
            Auth::Dashboard(_, allow_list) => { 
                if let Some((plugin, action)) = action.action.get_hashes() {
                    if allow_list.iter().find(|item| item.plugin == plugin && item.action == action).is_some() {
                        0
                    } else {
                        return Err("This action is not authorized for this dashboard".to_string());
                    }
                } else {
                    return Err("Invalid Action Format".to_string());
                }
            },
            Auth::Plugin(id, _) => id
        };

        let ds_r = self.datastore.read().await;
        let res = ds_r.trigger_web_action(origin, action).await;
        drop(ds_r);

        res.map_err(|e| e.to_string())
    }
}

/// Serves as the Messaging Protocol of the Socket.io Server Channel
pub(crate) enum SocketChMsg {
    AddDashboard(String),
    RmDashboard(String),
    ChangedProperty(crate::PropertyHandle, crate::utils::ValueContainer)
}

impl From<PropertyHandle> for WebPropertyHandle {
    fn from(value: PropertyHandle) -> Self {
        WebPropertyHandle::new(value.plugin, value.property)
    }
}

impl From<ActionHandle> for WebActionHandle {
    fn from(value: ActionHandle) -> Self {
        WebActionHandle::new(value.plugin, value.action)
    }
}

impl TryFrom<WebActionHandle> for ActionHandle {
    type Error = &'static str;

    fn try_from(value: WebActionHandle) -> Result<Self, Self::Error> {
        let (plugin, action) = value.get_hashes().ok_or("Malformed ActionHandle")?;
        Ok(ActionHandle { plugin, action })
    }
}

#[derive(Debug, Clone)]
pub(crate) struct IpMatcher {
    /// Ip address and mask, mask == 0 is just equals
    ipv4_rules: Vec<(Ipv4Addr, u32)>,

    /// Ip address and mask, mask == 0 is just equals
    ipv6_rules: Vec<(Ipv6Addr, u32)>,
}

impl IpMatcher {
    pub(crate) fn new(rules: Vec<String>) -> Result<Option<IpMatcher>, String> {
        let mut res = Self { ipv4_rules: Vec::new(), ipv6_rules: Vec::new() };

        for item in rules {

            let (ip_str, mask) = match item.split_once('/') {
                Some((ip, mask)) => {
                    let mask = mask.parse::<u32>().map_err(|e| format!("'{}' {e}", item.as_str()))?;
                    (ip,mask)
                },
                None => {
                    (item.as_str(), u32::MAX)
                }
            };

            match ip_str.parse::<IpAddr>() {
                Ok(ip) => {
                    match ip {
                        IpAddr::V4(v4) => {
                            let value = (v4, if mask == u32::MAX { 32 } else { mask });

                            if value.1 > 32 {
                                return Err(format!("'{}' a subnet mask of {mask} is invalid, range of 0 to 32 (both inclusive)", item.as_str()));
                            }

                            if !res.ipv4_rules.contains(&value) {
                                res.ipv4_rules.push(value);
                            }
                        },
                        IpAddr::V6(v6) => {
                            let value = (v6, if mask == u32::MAX { 128 } else { mask });

                            if value.1 > 128 {
                                return Err(format!("'{}' a subnet mask of {mask} is invalid, range of 0 to 128 (both inclusive)", item.as_str()));
                            }

                            if !res.ipv6_rules.contains(&value) {
                                res.ipv6_rules.push(value)
                            }
                        }
                    }
                },
                Err(e) => {
                    if item.eq_ignore_ascii_case("localhost") {
                        if !res.ipv4_rules.contains(&(Ipv4Addr::LOCALHOST,32)) {
                            res.ipv4_rules.push((Ipv4Addr::LOCALHOST, 32));
                        }
                        if !res.ipv6_rules.contains(&(Ipv6Addr::LOCALHOST,128)) {
                            res.ipv6_rules.push((Ipv6Addr::LOCALHOST, 128));
                        }
                    } else {
                        return Err(format!("'{}' {e}", item.as_str()));
                    }
                }
            };
        }

        if res.ipv4_rules.is_empty() && res.ipv6_rules.is_empty() {
            Ok(None)
        } else {
            Ok(Some(res))
        }
    }
}

impl IpMatchPerformer<IpAddr> for Option<IpMatcher> {
    fn perform(&self, value: &IpAddr) -> bool {
        match self {
            Some(s) => s.perform(value),
            None => true
        }
    }
}

impl IpMatchPerformer<IpAddr> for IpMatcher {
    fn perform(&self, value: &IpAddr) -> bool {
        match value {
            IpAddr::V4(v4) => self.perform(v4),
            IpAddr::V6(v6) => self.perform(v6),
        }
    }
}

impl IpMatchPerformer<Ipv4Addr> for IpMatcher {
    fn perform(&self, value: &Ipv4Addr) -> bool {
        for (ip, mask) in self.ipv4_rules.iter() {
            // Straight match
            if mask == &32 {
                if value == ip {
                    return true;
                }
            } else {
                // Masked match
                
                let value = value.clone().to_bits().unbounded_shr(*mask);
                let ip = ip.clone().to_bits().unbounded_shr(*mask);

                if value == ip {
                    return true;
                }
            }
        }

        false
    }
}

impl IpMatchPerformer<Ipv6Addr> for IpMatcher {
    fn perform(&self, value: &Ipv6Addr) -> bool {
        for (ip, mask) in self.ipv6_rules.iter() {
            // Straight match
            if mask == &128 {
                if value == ip {
                    return true;
                }
            } else {
                // Masked match
                
                let value = value.clone().to_bits().unbounded_shr(*mask);
                let ip = ip.clone().to_bits().unbounded_shr(*mask);

                if value == ip {
                    return true;
                }
            }
        }

        false
    }
}

pub(crate) trait IpMatchPerformer<T> {
    fn perform(&self, value: &T) -> bool;
}
