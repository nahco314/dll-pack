use crate::resolve::{resolve, ResolveError};
use crate::type_utils::{Caller, IOToFn};
use anyhow::{anyhow, Result};
use libloading::os::unix::{Library as LLNativeLibrary, Symbol, RTLD_LOCAL, RTLD_NOW};
use log::{debug, trace};
use std::fs;
use std::ops::Deref;
use std::path::PathBuf;
use url::Url;
use wasmtime::{Config, Engine, Instance as WasmInstance, Linker, Module, Store, TypedFunc};
use wasmtime_wasi::preview1::WasiP1Ctx;
use wasmtime_wasi::{preview1, DirPerms, FilePerms, WasiCtxBuilder};

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
                let Library::WasmLibrary(WasmLibrary { store, .. }) = library else {
                    panic!("Wasm function cannot be called without Wasm library");
                };
                <TypedFunc<Args, Res>>::call(func, store, args).unwrap()
            }
        }
    }
}

pub struct NativeLibrary {
    pub raw_library: LLNativeLibrary,
    pub raw_dependencies: Vec<LLNativeLibrary>,
}

pub struct WasmLibrary {
    pub instance: WasmInstance,
    pub store: Store<WasiP1Ctx>,
}

pub enum Library {
    NativeLibrary(NativeLibrary),
    WasmLibrary(WasmLibrary),
}

impl Library {
    pub(crate) fn new_native_library(
        raw_library: LLNativeLibrary,
        raw_dependencies: Vec<LLNativeLibrary>,
    ) -> Self {
        Library::NativeLibrary(NativeLibrary {
            raw_library,
            raw_dependencies,
        })
    }

    pub(crate) fn new_wasm_library(instance: WasmInstance, store: Store<WasiP1Ctx>) -> Self {
        Library::WasmLibrary(WasmLibrary { instance, store })
    }

    pub fn get_function<Args, Res>(&mut self, name: &str) -> Result<Function<Args, Res>>
    where
        Args: wasmtime::WasmParams,
        Res: wasmtime::WasmResults,
        (Args, Res): IOToFn,
        Args: Caller<Args, Res>,
    {
        match self {
            Library::NativeLibrary(NativeLibrary {
                raw_library: lib, ..
            }) => {
                let symbol: Symbol<<(Args, Res) as IOToFn>::Output> =
                    unsafe { lib.get(name.as_bytes())? };
                Ok(Function::LLFunction(symbol))
            }
            Library::WasmLibrary(WasmLibrary { instance, store }) => {
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
            base_info.path.display()
        );

        let wasm_bin = fs::read(&base_info.path)?;
        let module = Module::from_binary(&engine, wasm_bin.as_slice())?;

        let cache_bin = module.serialize()?;

        trace!("serializing to cache: {}", cache_path.display());

        fs::create_dir_all(cache_path.parent().unwrap())?;
        fs::write(&cache_path, cache_bin)?;

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

    Ok(Library::new_wasm_library(instance, store))
}

#[cfg(unix)]
unsafe fn libloading_load(path: &PathBuf) -> Result<LLNativeLibrary> {
    LLNativeLibrary::open(Some(path), RTLD_NOW | RTLD_LOCAL).map_err(|e| e.into())
}

#[cfg(windows)]
unsafe fn libloading_load(path: &PathBuf) -> Result<LLNativeLibrary> {
    LLNativeLibrary::new(path).map_err(|e| e.into())
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

    Ok(Library::new_native_library(lib, dependency_libs))
}

pub fn load(url: &Url, work_dir: &PathBuf) -> Result<Library> {
    let this_platform = env!("TARGET_TRIPLE");
    let with_this_platform = load_with_platform(url, work_dir, this_platform);

    let res = match with_this_platform {
        Ok(v) => v,
        Err(e) => {
            if let Some(m) = e.downcast_ref::<ResolveError>() {
                debug!("Failed to load with this platform: {}", m);

                load_with_wasm(url, work_dir, "wasm32-wasip1")?
            } else {
                return Err(e);
            }
        }
    };

    debug!("loaded: {}", url);

    Ok(res)
}
