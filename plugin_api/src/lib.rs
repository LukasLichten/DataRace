mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[no_mangle]
pub extern "C" fn run() {
    println!("HELLO EVERYNYAN!");
}
