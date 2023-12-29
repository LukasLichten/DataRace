// A lot of this is due to importing large sets of C standard lib
#[allow(dead_code,non_upper_case_globals,non_camel_case_types,improper_ctypes,non_snake_case)]
mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

#[cfg(feature = "main-entry")]
pub use bindings::run;

pub use bindings::PluginHandle;

pub use bindings::{log_info, log_error};
