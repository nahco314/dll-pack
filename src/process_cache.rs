use crate::load::{load_with_platform, Library};
use crate::resolve::{resolve, ResolveError};
use anyhow::anyhow;
use anyhow::Result;
use libloading::os::unix::{RTLD_LOCAL, RTLD_NOW};
use log::{debug, trace};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use url::Url;
use wasmtime::{Config, Engine, Linker, Module, Store};
use wasmtime_wasi::{preview1, DirPerms, FilePerms, WasiCtxBuilder};

/// Represents a unique source of a library, identified by its URL and platform.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct Source {
    url: Url,
    platform: String,
}

static CACHE: LazyLock<Mutex<HashMap<Source, Library>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Attempts to load a library from the cache or loads it anew if not present.
/// Executes a provided function with the library.
pub fn run_cached_load_with_platform<T>(
    url: &Url,
    work_dir: &PathBuf,
    platform: &str,
    run: impl Fn(&mut Library) -> Result<T>,
) -> Result<T> {
    let source = Source {
        url: url.clone(),
        platform: platform.to_string(),
    };

    let mut cache = CACHE.lock().unwrap();
    if let Some(lib) = cache.get_mut(&source) {
        debug!("loading cache: {}", url);

        return run(lib);
    }
    drop(cache);

    let mut lib = load_with_platform(url, work_dir, platform)?;

    let res = run(&mut lib);

    let mut cache = CACHE.lock().unwrap();
    cache.insert(source, lib);
    drop(cache);

    res
}

/// Tries to load a library using the current platform, and if it fails due to a `ResolveError`,
/// attempts to load it using a fallback platform.
fn run_cached_load_impl<T>(
    url: &Url,
    work_dir: &PathBuf,
    run: &impl Fn(&mut Library) -> Result<T>,
) -> Result<T> {
    let this_platform = env!("TARGET_TRIPLE");
    let with_this_platform = run_cached_load_with_platform(url, work_dir, this_platform, run);

    match with_this_platform {
        Ok(v) => Ok(v),
        Err(e) => {
            if let Some(m) = e.downcast_ref::<ResolveError>() {
                debug!("Failed to load with this platform: {}", m);

                run_cached_load_with_platform(url, work_dir, "wasm32-wasip1", run)
            } else {
                Err(e)
            }
        }
    }
}

/// A public function that wraps `run_cached_load_impl`, providing a simpler interface
/// for loading libraries with caching.
pub fn run_cached_load<T>(
    url: &Url,
    work_dir: &PathBuf,
    run: impl Fn(&mut Library) -> Result<T>,
) -> Result<T> {
    run_cached_load_impl(url, work_dir, &run)
}
