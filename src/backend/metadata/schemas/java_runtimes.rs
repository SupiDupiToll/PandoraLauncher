use std::{borrow::Cow, collections::HashMap, fmt::Display, path::{Path, PathBuf}, sync::Arc};

use chrono::{DateTime, Utc};
use gpui::SharedString;
use indexmap::IndexMap;
use serde::{de::DeserializeOwned, Deserialize};
use sha1::{Digest, Sha1};
use tokio::{runtime::Handle, sync::mpsc::Sender, task::JoinHandle};

#[derive(Deserialize, Debug)]
pub struct JavaRuntimes {
    #[serde(flatten)]
    pub platforms: HashMap<String, JavaRuntimePlatform>
}

#[derive(Deserialize, Debug)]
pub struct JavaRuntimePlatform {
    #[serde(flatten)]
    pub components: HashMap<String, Vec<JavaRuntimeComponent>>
}

#[derive(Deserialize, Debug)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct JavaRuntimeComponent {
    pub availability: JavaRuntimeComponentAvailability,
    pub manifest: JavaRuntimeComponentManifestLink,
    pub version: JavaRuntimeComponentVersion
}

#[derive(Deserialize, Debug)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct JavaRuntimeComponentManifestLink {
    pub sha1: SharedString,
    pub size: u32,
    pub url: SharedString
}

#[derive(Deserialize, Debug)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct JavaRuntimeComponentVersion {
    pub name: SharedString,
    pub released: DateTime<Utc>
}

#[derive(Deserialize, Debug)]
#[cfg_attr(debug_assertions, serde(deny_unknown_fields))]
pub struct JavaRuntimeComponentAvailability {
    pub group: u32,
    pub progress: u32
}

