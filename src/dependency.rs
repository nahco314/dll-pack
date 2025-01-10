use serde::{Deserialize, Serialize};
use url::Url;

/// Represents a dependency that can be loaded by the dll-pack system.
/// Dependencies can either be raw library files or packaged dllpack files that may contain
/// their own nested dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Dependency {
    /// A raw library file (.dll, .so, etc.) that can be loaded directly.
    /// These are typically platform-specific binary files without any metadata
    /// about their own dependencies.
    #[serde(rename = "rawlib")]
    RawLib {
        /// URL where the library file can be downloaded from
        #[serde(with = "url_serde")]
        url: Url,
        /// Optional name to identify this library.
        /// If not provided, the filename from the URL will be used
        #[serde(default)]
        name: Option<String>,
    },

    /// A packaged dllpack file that contains a library along with its manifest
    /// describing platform-specific dependencies. This allows for recursive
    /// dependency resolution.
    #[serde(rename = "dllpack")]
    DllPack {
        /// URL where the .dllpack file can be downloaded from
        #[serde(with = "url_serde")]
        url: Url,
    },
}
