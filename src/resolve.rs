use crate::dependency::Dependency;
use crate::dllpack_file::{DllPackFile, PlatformManifest};
use crate::download::{cached_download_lib, cached_download_manifest, DllInfo, ManifestInfo};
use anyhow::{Result};
use std::collections::BTreeMap;
use std::fmt::Display;
use std::path::PathBuf;
use url::Url;

#[derive(Debug)]
pub enum ResolveError {
    PlatformNotSupported(String),
}

impl Into<anyhow::Error> for ResolveError {
    fn into(self) -> anyhow::Error {
        anyhow::anyhow!(self)
    }
}

impl Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolveError::PlatformNotSupported(platform) => {
                write!(f, "Platform {} is not supported", platform)
            }
        }
    }
}

fn fetch_manifests_inner(
    base_info: &ManifestInfo,
    work_dir: &PathBuf,
    platform: &str,
    result_map: &mut BTreeMap<ManifestInfo, PlatformManifest>,
    dependency_map: &mut BTreeMap<ManifestInfo, Vec<ManifestInfo>>,
    reverse_dependency_map: &mut BTreeMap<ManifestInfo, Vec<ManifestInfo>>,
) -> Result<()> {
    cached_download_manifest(&base_info)?;

    let file = DllPackFile::from_file(&base_info.path)?;
    let manifest = file.manifest;

    let Some(p_manifest) = manifest.platforms.get(platform) else {
        return Err(anyhow::anyhow!(ResolveError::PlatformNotSupported(
            platform.to_string()
        )));
    };

    result_map.insert(base_info.clone(), p_manifest.clone());

    let mut deps = Vec::new();

    for dep in &p_manifest.dependencies {
        match dep {
            Dependency::DllPack { url } => {
                let info = ManifestInfo::from_input(url, work_dir)?;
                deps.push(info.clone());

                if !result_map.contains_key(&info) {
                    fetch_manifests_inner(
                        &info,
                        work_dir,
                        platform,
                        result_map,
                        dependency_map,
                        reverse_dependency_map,
                    )?;
                }

                reverse_dependency_map
                    .entry(info.clone())
                    .or_insert_with(Vec::new)
                    .push(base_info.clone());
            }
            _ => {}
        }
    }

    dependency_map.insert(base_info.clone(), deps);

    Ok(())
}

fn fetch_manifests(
    base_url: &Url,
    work_dir: &PathBuf,
    platform: &str,
) -> Result<(
    ManifestInfo,
    BTreeMap<ManifestInfo, PlatformManifest>,
    BTreeMap<ManifestInfo, Vec<ManifestInfo>>,
    BTreeMap<ManifestInfo, Vec<ManifestInfo>>,
)> {
    let mut result_map = BTreeMap::new();
    let mut dependency_map = BTreeMap::new();
    let mut reverse_dependency_map = BTreeMap::new();

    let base_info = ManifestInfo::from_input(base_url, work_dir)?;

    fetch_manifests_inner(
        &base_info,
        work_dir,
        platform,
        &mut result_map,
        &mut dependency_map,
        &mut reverse_dependency_map,
    )?;

    Ok((
        base_info,
        result_map,
        dependency_map,
        reverse_dependency_map,
    ))
}

pub fn resolve(
    base_url: &Url,
    work_dir: &PathBuf,
    platform: &str,
) -> Result<(DllInfo, Vec<DllInfo>)> {
    let (base_info, result_map, dependency_map, reverse_dependency_map) =
        fetch_manifests(base_url, work_dir, platform)?;

    let mut available = Vec::new();
    let mut remain_deps_counts =
        BTreeMap::from_iter(dependency_map.iter().map(|(k, v)| (k.clone(), v.len())));
    let mut unresolved_count = result_map.len();
    let mut dependency_load_order = Vec::new();

    for (m_info, count) in remain_deps_counts.iter() {
        if count == &0 {
            available.push(m_info.clone());
            unresolved_count -= 1;
            if &m_info.url != base_url {
                dependency_load_order.push(m_info.clone());
            }
        }
    }

    while !available.is_empty() {
        let url = available.pop().unwrap();

        for dep in reverse_dependency_map.get(&url).unwrap_or(&vec![]) {
            let count = remain_deps_counts.get_mut(dep).unwrap();
            *count -= 1;

            if *count == 0 {
                available.push(dep.clone());
                unresolved_count -= 1;

                if &dep.url != base_url {
                    dependency_load_order.push(dep.clone());
                }
            }
        }
    }

    if unresolved_count > 0 {
        return Err(anyhow::anyhow!(
            "Failed to resolve all dependencies for {}. It may be a circular dependency.",
            platform
        ));
    }

    let mut dependency_load_order_paths = Vec::new();

    for m_info in dependency_load_order.iter() {
        let manifest = result_map.get(m_info).unwrap();

        let dll_info = DllInfo::from_input(
            &manifest.url,
            &manifest.name.as_ref().map(String::as_str),
            work_dir,
        )?;
        cached_download_lib(&dll_info)?;
        dependency_load_order_paths.push(dll_info);
    }

    let manifest = result_map.get(&base_info).unwrap();
    let dll_info = DllInfo::from_input(
        &manifest.url,
        &manifest.name.as_ref().map(String::as_str),
        work_dir,
    )?;
    cached_download_lib(&dll_info)?;

    Ok((dll_info, dependency_load_order_paths))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn test_resolve() {
        let work_dir = PathBuf::from_str("/home/nahco314/RustroverProjects/dll-pack/work").unwrap();
        let base_url = Url::from_str("http://0.0.0.0:8000/a.dllpack").unwrap();
        let platform = "wasm32-wasip1";

        let result = resolve(&base_url, &work_dir, platform).unwrap();

        println!("{:?}", result);
    }
}
