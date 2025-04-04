use serde::{Deserialize, Serialize};

pub type UpdatePackage = Vec<(PropertyHandle, Value)>;

/// Special format for PropertyHandles in web use, where the two hashes are concatenated into a
/// string. Use [`PropertyHandle::new(plugin, prop)`] to convert hashes to web, and
/// [`handle.get_hashes()`] converts them back.
///
/// Part of the reason why is because js maps do not work with objects (objects were all fields are
/// the same value are not equal), and because the numbers in jsons are anyway just unicode symbols
/// might as well send them as a String
#[derive(Debug,Clone,Deserialize,Serialize)]
#[serde(transparent)]
pub struct PropertyHandle(String);

impl PropertyHandle {
    pub fn new(plugin: u64, prop: u64) -> Self {
        Self(format!("{}|{}", plugin, prop))
    }

    pub fn get_hashes(&self) -> Option<(u64, u64)> {
        extract_hashes(self.0.as_str())
    }
}

/// Special format for ActionHandle in web use, where the two hashes are concatenated into a
/// string. Use [`PropertyHandle::new(plugin, prop)`] to convert hashes to web, and
/// [`handle.get_hashes()`] converts them back.
#[derive(Debug,Clone,Deserialize,Serialize)]
#[serde(transparent)]
pub struct ActionHandle(String);

impl ActionHandle {
    pub fn new(plugin: u64, action: u64) -> Self {
        Self(format!("{}|{}", plugin, action))
    }

    pub fn get_hashes(&self) -> Option<(u64, u64)> {
        extract_hashes(self.0.as_str())
    }
}

fn extract_hashes(web_handle: &str) -> Option<(u64, u64)> {
    let (plugin,specific) = web_handle.split_once('|')?;

    let plugin: u64 = plugin.parse().ok()?;
    let specific: u64 = specific.parse().ok()?;

    Some((plugin, specific))
}

/// A single Value, but for internal and web use
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum Value {
    None,
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),

    // It is of note: js auto converts to doubles (52bit mantise),
    // so messauring in microseconds means we start loosing precision already after ~140 years.
    // Frankly, js will likely have switched to 128bit floats at that point
    //
    // Otherwise, rewrite all js code to expect duration to be in seconds and...
    // You know, doesn't matter, same precision issue, you will eventually loose the microsecond
    // precision, although if your number reads over 100years I think you have different
    // priorities, and while internally i64 hard caps Duration to 500k years, js can handle more.
    Dur(i64),

    Arr(Vec<Value>),
    ArrUpdate(Vec<(usize, Value)>)
}

/// A single Action, for web use
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub action: ActionHandle,
    pub param: Option<Vec<Value>>
}

