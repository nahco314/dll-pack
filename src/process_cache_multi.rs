use crate::load::{load_with_platform, Library};
use crate::resolve::ResolveError;
use anyhow::Result;
use log::debug;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, Mutex, RwLock};
use url::Url;

/// Represents a unique source of a library, identified by its URL and platform.
///
/// In the multi-resource approach, each key has a "pool" of `Library` instances.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
struct Source {
    url: Url,
    platform: String,
}

/// A pool of `Library` instances associated with a specific `Source`.
///
/// Whenever a library is requested and `available` is empty, we create a new one.
/// By doing so, multiple threads can request the same `Source` simultaneously
/// without blocking each other on a single `Library`.
struct ResourcePool {
    available: Vec<Library>,
    in_use_count: usize,
}

impl ResourcePool {
    fn new() -> Self {
        Self {
            available: Vec::new(),
            in_use_count: 0,
        }
    }

    /// Fetch or create a new `Library`. If the pool is out of idle libraries,
    /// we'll load a new `Library`.
    fn get_or_create_resource(
        &mut self,
        source: &Source,
        work_dir: &PathBuf,
        platform: &str,
    ) -> Result<Library> {
        if let Some(lib) = self.available.pop() {
            debug!("MULTI CACHE: reusing existing Library for {}", source.url);
            self.in_use_count += 1;
            Ok(lib)
        } else {
            debug!("MULTI CACHE: creating new Library for {}", source.url);
            let lib = load_with_platform(&source.url, work_dir, platform)?;
            self.in_use_count += 1;
            Ok(lib)
        }
    }

    /// Return a borrowed `Library` to the pool so it can be reused.
    fn return_resource(&mut self, lib: Library) {
        self.available.push(lib);
        self.in_use_count -= 1;
    }
}

/// Global multi-resource cache: `Source -> Arc<Mutex<ResourcePool>>`.
///
/// - We use `RwLock` around the `HashMap` so multiple threads can read
///   concurrently when finding a `Source`.
/// - Each `ResourcePool` is behind a `Mutex` to guard the internal vectors
///   and counters when we borrow or return a `Library`.
static MULTI_CACHE: LazyLock<RwLock<HashMap<Source, Arc<Mutex<ResourcePool>>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// A guard that holds a borrowed `Library` from the pool. When dropped, it
/// automatically returns the `Library` to that pool.
pub struct ResourceGuard {
    source: Source,
    pool: Arc<Mutex<ResourcePool>>,
    library: Option<Library>,
}

impl ResourceGuard {
    fn new(source: Source, pool: Arc<Mutex<ResourcePool>>, library: Library) -> Self {
        Self {
            source,
            pool,
            library: Some(library),
        }
    }

    /// Returns a mutable reference to the underlying `Library`.
    /// Because the `Library` might be non-thread-safe, each thread
    /// must hold its own instance via this guard.
    pub fn library_mut(&mut self) -> &mut Library {
        self.library.as_mut().unwrap()
    }
}

impl Drop for ResourceGuard {
    fn drop(&mut self) {
        if let Some(lib) = self.library.take() {
            let mut pool = self.pool.lock().unwrap();
            pool.return_resource(lib);
            debug!(
                "MULTI CACHE: returned library to the pool for {}",
                self.source.url
            );
        }
    }
}

/// Internal helper: fetch or create a `ResourcePool` for the given `Source`,
/// then borrow one `Library` from it (creating a new one if needed).
fn get_library_resource(
    source: &Source,
    work_dir: &PathBuf,
    platform: &str,
) -> Result<ResourceGuard> {
    // Step 1: look for an existing pool; if not found, create it.
    let pool_arc = {
        let read_map = MULTI_CACHE.read().unwrap();
        if let Some(pool) = read_map.get(source) {
            Arc::clone(pool)
        } else {
            drop(read_map);
            let mut write_map = MULTI_CACHE.write().unwrap();
            let entry = write_map
                .entry(source.clone())
                .or_insert_with(|| Arc::new(Mutex::new(ResourcePool::new())));
            Arc::clone(entry)
        }
    };

    // Step 2: borrow one `Library` from the pool
    let mut pool = pool_arc.lock().unwrap();
    let lib = pool.get_or_create_resource(source, work_dir, platform)?;
    drop(pool);

    Ok(ResourceGuard::new(source.clone(), pool_arc, lib))
}

/// Public function that attempts to load a library from the multithreaded cache
/// or loads it anew if not present. Executes the provided function using
/// a mutable reference to the borrowed `Library`.
///
/// This is the recommended entry point if you want to allow multiple threads
/// to use the same `Source` concurrently, each with its own `Library`.
pub fn run_multi_cached_with_platform<T>(
    url: &Url,
    work_dir: &PathBuf,
    platform: &str,
    run: impl Fn(&mut Library) -> Result<T>,
) -> Result<T> {
    let source = Source {
        url: url.clone(),
        platform: platform.to_string(),
    };

    // Acquire a `ResourceGuard` from the multi-resource pool.
    let mut guard = get_library_resource(&source, work_dir, platform)?;

    // Execute the user-provided closure
    run(guard.library_mut())
}

/// Internal fallback logic: tries the current platform, then falls back to "wasm32-wasip1".
fn run_multi_cached_impl<T>(
    url: &Url,
    work_dir: &PathBuf,
    run: &impl Fn(&mut Library) -> Result<T>,
) -> Result<T> {
    let this_platform = env!("TARGET_TRIPLE");
    match run_multi_cached_with_platform(url, work_dir, this_platform, run) {
        Ok(v) => Ok(v),
        Err(e) => {
            if let Some(res_err) = e.downcast_ref::<ResolveError>() {
                debug!(
                    "MULTI CACHE: failed with {}, fallback to wasm32-wasip1",
                    res_err
                );
                run_multi_cached_with_platform(url, work_dir, "wasm32-wasip1", run)
            } else {
                Err(e)
            }
        }
    }
}

/// A public entry point for the multi-resource cache.
/// This is analogous to the single-resource entry point but uses a pool-based
/// approach under the hood.
pub fn run_multi_cached<T>(
    url: &Url,
    work_dir: &PathBuf,
    run: impl Fn(&mut Library) -> Result<T>,
) -> Result<T> {
    run_multi_cached_impl(url, work_dir, &run)
}
