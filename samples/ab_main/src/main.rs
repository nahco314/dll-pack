use anyhow::Result;
use libloading;
use libloading::os::unix::{RTLD_LAZY, RTLD_LOCAL, RTLD_NOW};

fn main() -> Result<()> {
    let b_lib = unsafe {
        libloading::os::unix::Library::open(
            Some("/home/nahco314/RustroverProjects/dll-pack/samples/b_lib/target/release/libb_lib.so"),
            RTLD_LAZY | RTLD_LOCAL,
        )
    }?;
    let a_lib = unsafe {
        libloading::os::unix::Library::open(
            Some("/home/nahco314/RustroverProjects/dll-pack/samples/a_lib/target/release/liba_lib.so"),
            RTLD_LAZY | RTLD_LOCAL,
        )
    }?;

    let call_hello_world: libloading::os::unix::Symbol<unsafe extern "C" fn()> =
        unsafe { a_lib.get(b"call_hello_world")? };

    unsafe {
        call_hello_world();
    }

    Ok(())
}
