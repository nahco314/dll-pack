use crate::dependency::Dependency;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use url::Url;

/// Information about DLLs on a specific platform.
/// Here, “platform” refers to a target triple used by Rust for building,
/// such as `x86_64-unknown-linux-gnu.`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformManifest {
    /// URL where the library file can be downloaded from
    #[serde(with = "url_serde")]
    pub url: Url,

    /// Optional name to identify this library.
    /// If not provided, the filename from the URL will be used
    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

/// A struct that stores a PlatformManifest for each respective platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub platforms: BTreeMap<String, PlatformManifest>,
}

///　A struct corresponding to the top level of a dllpack file,
/// consisting of a `Manifest` that stores specific information and
/// a `spec_version` that indicates the version of the dllpack file specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DllPackFile {
    /// A string representing the version of the dllpack file specification.
    /// This field exists for backward compatibility,
    /// ensuring that even if there are breaking changes to the dllpack specification in the future,
    /// older versions of the file can still be identified and parsed.
    ///
    /// Note that currently, no value other than "1.0.0" exists or is allowed.
    #[serde(rename = "spec-version")]
    pub spec_version: String,

    pub manifest: Manifest,
}

impl DllPackFile {
    pub fn from_str(s: &str) -> Result<Self> {
        let res: DllPackFile = serde_json::from_str(s)?;
        if res.spec_version != "1.0.0" {
            return Err(anyhow!("Unsupported spec version: {}", res.spec_version));
        }

        Ok(res)
    }

    pub fn to_string(&self) -> Result<String> {
        serde_json::to_string(self).map_err(Into::into)
    }

    pub fn from_file<P: AsRef<std::path::Path>>(path: P) -> Result<Self> {
        let s = std::fs::read_to_string(path)?;
        Self::from_str(&s)
    }
}
