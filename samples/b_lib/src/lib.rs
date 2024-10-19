#[no_mangle]
pub extern "C" fn hello_world() -> *const u8 {
    let s = "Hello, World!\0";
    s.as_ptr()
}
