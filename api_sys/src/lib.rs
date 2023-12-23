mod ex {
    extern "C" {
        pub(crate) fn run();
    }
}

pub fn run() {
    unsafe {
        ex::run();
    }
}
