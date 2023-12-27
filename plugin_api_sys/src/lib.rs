#[allow(dead_code)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

#[cfg(feature = "main-entry")]
pub use bindings::run;

pub use bindings::PluginHandle;

pub use bindings::{log_info, log_error};
