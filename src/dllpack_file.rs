use std::collections::BTreeMap;
use crate::dependency::Dependency;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformManifest {
    #[serde(with = "url_serde")]
    pub url: Url,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<Dependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub platforms: BTreeMap<String, PlatformManifest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DllPackFile {
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
