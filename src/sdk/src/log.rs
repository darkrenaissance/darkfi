/// Print a message to the log
#[macro_export]
macro_rules! msg {
    ($msg:expr) => {
        $crate::log::drk_log($msg)
    };
    ($($arg:tt)*) => ($crate::log::drk_log(&format!($($arg)*)));
}

#[inline]
pub fn drk_log(message: &str) {
    #[cfg(target_arch = "wasm32")]
    unsafe {
        drk_log_(message.as_ptr(), message.len());
    }

    #[cfg(not(target_arch = "wasm32"))]
    println!("{}", message);
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn drk_log_(ptr: *const u8, len: usize);
}
