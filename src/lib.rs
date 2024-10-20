use crate::download::DllInfo;
use crate::resolve::{resolve, ResolveError};
use crate::type_utils::{Caller, IOToFn};
use anyhow::{anyhow, Result};
#[cfg(unix)]
use libloading::os::unix::{Library as LLLibrary, Symbol, RTLD_LOCAL, RTLD_NOW};
#[cfg(windows)]
use libloading::os::windows::{Library as LLLibrary, Symbol};
use log::{debug, info, trace};
use std::ops::Deref;
use std::path::PathBuf;
use std::{fs, marker};
use url::Url;
use wasmtime::{
    Config, Engine, Instance as WasmInstance, IntoFunc, Linker, Module as WasmModule, Module,
    Store, TypedFunc,
};
use wasmtime_wasi::preview1::WasiP1Ctx;
use wasmtime_wasi::{preview1, DirPerms, FilePerms, WasiCtxBuilder};

mod dependency;
mod dllpack_file;
mod download;
mod resolve;
mod type_utils;

pub enum Function<Args, Res>
where
    (Args, Res): IOToFn,
{
    LLFunction(Symbol<<(Args, Res) as IOToFn>::Output>),
    WasmFunction(TypedFunc<Args, Res>),
}

impl<Args, Res> Function<Args, Res>
where
    Args: wasmtime::WasmParams,
    Res: wasmtime::WasmResults,
    (Args, Res): IOToFn,
    Args: Caller<Args, Res>,
{
    pub fn call(&self, library: &mut Library, args: Args) -> Res {
        match &self {
            Function::LLFunction(symbol) => unsafe {
                let a = symbol.deref();
                <Args as Caller<Args, Res>>::call(args, a)
            },
            Function::WasmFunction(func) => {
                let Library::WasmLibrary(_, store) = library else {
                    panic!("Wasm function cannot be called without Wasm library");
                };
                <TypedFunc<Args, Res>>::call(func, store, args).unwrap()
            }
        }
    }
}

pub enum Library {
    LLLibrary(LLLibrary, Vec<LLLibrary>),
    WasmLibrary(WasmInstance, Store<WasiP1Ctx>),
}

impl Library {
    pub fn get_function<Args, Res>(&mut self, name: &str) -> Result<Function<Args, Res>>
    where
        Args: wasmtime::WasmParams,
        Res: wasmtime::WasmResults,
        (Args, Res): IOToFn,
        Args: Caller<Args, Res>,
    {
        match self {
            Library::LLLibrary(lib, _) => {
                let symbol: Symbol<<(Args, Res) as IOToFn>::Output> =
                    unsafe { lib.get(name.as_bytes())? };
                Ok(Function::LLFunction(symbol))
            }
            Library::WasmLibrary(instance, store) => {
                let func = instance.get_typed_func::<Args, Res>(store, name)?;
                Ok(Function::WasmFunction(func))
            }
        }
    }
}

fn is_wasm(platform: &str) -> bool {
    platform.contains("wasm")
}

fn is_wasi(platform: &str) -> bool {
    platform.contains("wasi")
}

pub fn load_with_wasm(url: &Url, work_dir: &PathBuf, platform: &str) -> Result<Library> {
    debug!("toplevel-load with {}: {}", platform, url);

    let (base_info, dependency_load_order_paths) = resolve(url, work_dir, platform)?;

    // basic wasm file cannot include dependencies
    // note : wasm component can include dependencies maybe
    if !dependency_load_order_paths.is_empty() {
        return Err(anyhow!("Wasm file cannot include dependencies"));
    }

    let mut config = Config::default();
    // https://github.com/bytecodealliance/wasmtime/issues/8897
    #[cfg(unix)]
    config.native_unwind_info(false);
    let engine = Engine::new(&config)?;

    let cache_path = base_info.wasm_module_cache_path();

    let module = if cache_path.exists() {
        debug!(
            "{}: loading from cache: {}",
            base_info.name,
            cache_path.display()
        );

        let module;
        unsafe {
            module = Module::deserialize_file(&engine, &cache_path)?;
        }

        module
    } else {
        debug!(
            "{}: manual loading: {}",
            base_info.name,
            cache_path.display()
        );

        let module = Module::from_file(&engine, base_info.path)?;
        let bin = module.serialize()?;

        fs::create_dir_all(cache_path.parent().unwrap())?;
        fs::write(&cache_path, bin)?;

        module
    };

    let mut linker = Linker::new(&engine);

    preview1::add_to_linker_sync(&mut linker, |t| t)?;
    let pre = linker.instantiate_pre(&module)?;

    let wasi_ctx = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_env()
        .preopened_dir("/", "/", DirPerms::all(), FilePerms::all())?
        .build_p1();

    let mut store = Store::new(&engine, wasi_ctx);
    let instance = pre.instantiate(&mut store)?;

    Ok(Library::WasmLibrary(instance, store))
}

#[cfg(unix)]
unsafe fn libloading_load(path: &PathBuf) -> Result<LLLibrary> {
    LLLibrary::open(Some(path), RTLD_NOW | RTLD_LOCAL).map_err(|e| e.into())
}

#[cfg(windows)]
unsafe fn libloading_load(path: &PathBuf) -> Result<LLLibrary> {
    LLLibrary::new(path).map_err(|e| e.into())
}

pub fn load_with_platform(url: &Url, work_dir: &PathBuf, platform: &str) -> Result<Library> {
    if is_wasm(platform) {
        return load_with_wasm(url, work_dir, platform);
    }

    debug!("toplevel-load with {}: {}", platform, url);

    let (base_info, dependency_load_order_paths) = resolve(url, work_dir, platform)?;
    let mut dependency_libs = Vec::new();

    for d in dependency_load_order_paths {
        trace!("loading dependency: {}", d.url);
        let lib = unsafe { libloading_load(&d.path)? };
        dependency_libs.push(lib);
    }

    trace!("loading base library: {}", base_info.url);
    let lib = unsafe { libloading_load(&base_info.path)? };

    Ok(Library::LLLibrary(lib, dependency_libs))
}

pub fn load(url: &Url, work_dir: &PathBuf) -> Result<Library> {
    let this_platform = env!("TARGET_TRIPLE");
    let with_this_platform = load_with_platform(url, work_dir, this_platform);

    match with_this_platform {
        Ok(v) => Ok(v),
        Err(e) => {
            if let Some(m) = e.downcast_ref::<ResolveError>() {
                debug!("Failed to load with this platform: {}", m);

                load_with_wasm(url, work_dir, "wasm32-wasip1")
            } else {
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn it_works() {
        let mut result = load_with_platform(
            &Url::from_str("http://0.0.0.0:8000/a.dllpack").unwrap(),
            &PathBuf::from_str("/home/nahco314/RustroverProjects/dll-pack/work").unwrap(),
            "x86_64-unknown-linux",
        )
        .unwrap();

        let a = result.get_function::<(i32, i32), (i32)>("adding").unwrap();
        let res = a.call(&mut result, (2, 3));

        println!("{}", res);
    }

    #[test]
    fn two() {
        let mut result = load_with_platform(
            &Url::from_str("https://github.com/nahco314/dll-pack-sample-adder/releases/download/v0.1.0/dll-pack-sample-adder.dllpack").unwrap(),
            &PathBuf::from_str("/home/nahco314/RustroverProjects/dll-pack/work").unwrap(),
            "x86_64-unknown-linux-gnu",
        )
            .unwrap();

        let a = result.get_function::<(i32, i32), (i32)>("adding_and_one").unwrap();
        let res = a.call(&mut result, (2, 3));

        println!("{}", res);
    }
}
