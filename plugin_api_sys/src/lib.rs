mod bindings {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

#[cfg(feature = "main-entry")]
pub unsafe fn run() {
    bindings::run();
}
