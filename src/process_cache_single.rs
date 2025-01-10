use crate::load::{load_with_platform, Library};
use crate::resolve::ResolveError;
use anyhow::Result;
use log::debug;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use url::Url;

/// This struct is the "key" in our cache (URL + platform).
/// In the single-resource approach, each key has exactly one `Library`.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct Source {
    url: Url,
    platform: String,
}

/// A global single-resource cache: `Source -> Library`.
///
/// - We use a `Mutex` for thread safety. Only one thread can modify
///   or read the internal `HashMap` at a time.
/// - This approach ensures that each `Source` has exactly one `Library`
///   in the cache.
static SINGLE_CACHE: LazyLock<Mutex<HashMap<Source, Library>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Loads (or reuses) a single `Library` for a given url and platform,
/// then executes a function on it.
///
/// This function is intended for use cases where each `Library` should not be used
/// concurrently (e.g., the library has internal state or side effects that are not thread-safe).
pub fn run_single_cached_with_platform<T>(
    url: &Url,
    work_dir: &PathBuf,
    platform: &str,
    run: impl Fn(&mut Library) -> Result<T>,
) -> Result<T> {
    // Build the "key" for the single cache
    let source = Source {
        url: url.clone(),
        platform: platform.to_string(),
    };

    // Lock the global cache
    let mut cache = SINGLE_CACHE.lock().unwrap();

    // Check if we already have a Library for this Source
    if let Some(lib) = cache.get_mut(&source) {
        debug!("SINGLE CACHE: found existing library for {}", source.url);
        return run(lib);
    }

    // Otherwise, load a new Library and insert it into the cache
    debug!("SINGLE CACHE: creating new library for {}", source.url);
    let mut lib = load_with_platform(url, work_dir, platform)?;
    let result = run(&mut lib);

    // Insert the library into the cache for future reuse
    cache.insert(source, lib);

    // Return the result of running the closure
    result
}

/// Internally attempts to load using the current platform, and if it fails
/// due to a `ResolveError`, falls back to "wasm32-wasip1".
fn run_single_cached_impl<T>(
    url: &Url,
    work_dir: &PathBuf,
    run: &impl Fn(&mut Library) -> Result<T>,
) -> Result<T> {
    let this_platform = env!("TARGET_TRIPLE");
    match run_single_cached_with_platform(url, work_dir, this_platform, run) {
        Ok(v) => Ok(v),
        Err(e) => {
            if let Some(res_err) = e.downcast_ref::<ResolveError>() {
                debug!(
                    "SINGLE CACHE: failed to load with {}, fallback to wasm32-wasip1",
                    res_err
                );
                run_single_cached_with_platform(url, work_dir, "wasm32-wasip1", run)
            } else {
                Err(e)
            }
        }
    }
}

/// A simple public function that uses the single-resource cache.
/// This is the recommended entry point if you know your library
/// can't safely run multiple instances concurrently for the same `Source`.
pub fn run_single_cached<T>(
    url: &Url,
    work_dir: &PathBuf,
    run: impl Fn(&mut Library) -> Result<T>,
) -> Result<T> {
    run_single_cached_impl(url, work_dir, &run)
}
