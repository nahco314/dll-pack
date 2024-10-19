use serde::{Deserialize, Serialize};
use url::Url;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Dependency {
    #[serde(rename = "rawlib")]
    RawLib {
        #[serde(with = "url_serde")]
        url: Url,
        #[serde(default)]
        name: Option<String>,
    },
    #[serde(rename = "dllpack")]
    DllPack {
        #[serde(with = "url_serde")]
        url: Url,
    },
}
