use std::{borrow::Cow, collections::HashMap, fmt::Display, path::{Path, PathBuf}, sync::Arc};

use chrono::{DateTime, Utc};
use gpui::SharedString;
use indexmap::IndexMap;
use serde::{de::DeserializeOwned, Deserialize};
use sha1::{Digest, Sha1};
use tokio::{runtime::Handle, sync::mpsc::Sender, task::JoinHandle};

#[derive(Deserialize, Clone, Debug)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct JavaRuntimeComponentManifest {
    pub files: IndexMap<Arc<Path>, JavaRuntimeComponentFile>
}

#[derive(Deserialize, Clone, Debug)]
#[serde(tag = "type")]
#[serde(rename_all = "lowercase")]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub enum JavaRuntimeComponentFile {
    Directory,
    File {
        executable: bool,
        downloads: JavaRuntimeComponentFileDownloads
    },
    Link {
        target: Arc<Path>
    }
}

#[derive(Deserialize, Clone, Debug)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct JavaRuntimeComponentFileDownloads {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lzma: Option<JavaRuntimeComponentFileDownload>,
    pub raw: JavaRuntimeComponentFileDownload
}

#[derive(Deserialize, Clone, Debug)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct JavaRuntimeComponentFileDownload {
    pub sha1: SharedString,
    pub size: u32,
    pub url: SharedString
}