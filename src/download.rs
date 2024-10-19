use anyhow::{anyhow, Result};
use log::{debug, trace};
use reqwest;
use std::fs;
use std::fs::DirBuilder;
use std::io::Write;
use std::path::PathBuf;
use url::Url;
use urlencoding;

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct DllInfo {
    pub url: Url,
    pub name: String,
    pub path: PathBuf,
}

impl DllInfo {
    pub fn new(url: Url, name: String, path: PathBuf) -> Self {
        Self { url, name, path }
    }

    pub fn from_input(url: &Url, name: &Option<&str>, dir_path: &PathBuf) -> Result<Self> {
        let e_url = urlencoding::encode(url.as_str());

        let last_of_url_path = url.path_segments().and_then(|s| s.last());
        let name = name
            .or(last_of_url_path)
            .ok_or(anyhow!("Could not get file name"))?;

        let path = dir_path.join(e_url.to_string()).join(name);

        Ok(Self::new(url.clone(), name.to_string(), path))
    }

    pub fn wasm_module_cache_path(&self) -> PathBuf {
        self.path
            .parent()
            .unwrap()
            .join(format!("module-cache-{}.bin", self.name))
    }
}

pub fn download_lib(dll_info: &DllInfo) -> Result<()> {
    debug!("downloading: {}", dll_info.path.display());

    let res = reqwest::blocking::get(dll_info.url.as_str())?;

    if !res.status().is_success() {
        return Err(anyhow!("Failed to download: {}", res.status()));
    }

    DirBuilder::new()
        .recursive(true)
        .create(dll_info.path.parent().unwrap())?;

    let mut file = fs::File::create(&dll_info.path)?;

    let content = res.bytes()?;
    file.write_all(&content)?;

    Ok(())
}

pub fn cached_download_lib(dll_info: &DllInfo) -> Result<()> {
    if dll_info.path.exists() {
        trace!("cached: {}", dll_info.path.display());

        return Ok(());
    }

    download_lib(dll_info)
}

#[derive(Debug, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub struct ManifestInfo {
    pub url: Url,
    pub path: PathBuf,
}

impl ManifestInfo {
    pub fn new(url: Url, path: PathBuf) -> Self {
        Self { url, path }
    }

    pub fn from_input(url: &Url, dir_path: &PathBuf) -> Result<Self> {
        let e_url = urlencoding::encode(url.as_str());
        let path = dir_path.join("_manifests").join(e_url.to_string());

        Ok(Self::new(url.clone(), path))
    }
}

pub fn download_manifest(manifest_info: &ManifestInfo) -> Result<()> {
    debug!("downloading: {}", manifest_info.path.display());

    let res = reqwest::blocking::get(manifest_info.url.as_str())?;

    if !res.status().is_success() {
        return Err(anyhow!("Failed to download: {}", res.status()));
    }

    DirBuilder::new()
        .recursive(true)
        .create(manifest_info.path.parent().unwrap())?;

    let mut file = fs::File::create(&manifest_info.path)?;

    let content = res.bytes()?;
    file.write_all(&content)?;

    Ok(())
}

pub fn cached_download_manifest(manifest_info: &ManifestInfo) -> Result<()> {
    if manifest_info.path.exists() {
        trace!("cached: {}", manifest_info.path.display());

        return Ok(());
    }

    download_manifest(manifest_info)
}
