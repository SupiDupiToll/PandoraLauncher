use std::{borrow::Cow, collections::HashMap, fmt::Display, path::{Path, PathBuf}, sync::Arc};

use chrono::{DateTime, Utc};
use gpui::SharedString;
use indexmap::IndexMap;
use serde::{de::DeserializeOwned, Deserialize};
use sha1::{Digest, Sha1};
use tokio::{runtime::Handle, sync::mpsc::Sender, task::JoinHandle};

use crate::{backend::metadata::schemas::{java_runtime_component::JavaRuntimeComponentManifest, java_runtimes::JavaRuntimes, version::MinecraftVersion, version_manifest::{MinecraftVersionLink, MinecraftVersionManifest, MOJANG_VERSION_MANIFEST_URL}}, bridge::MessageToFrontend};

type MetaLoadStateWrapper<T> = Arc<tokio::sync::Mutex<MetaLoadState<T>>>;

#[derive(Default)]
pub struct MetadataManagerStates {
    minecraft_version_manifest: MetaLoadStateWrapper<MinecraftVersionManifest>,
    mojang_java_runtimes: MetaLoadStateWrapper<JavaRuntimes>,
    version_info: std::sync::RwLock<HashMap<SharedString, MetaLoadStateWrapper<MinecraftVersion>>>,
    assets_index: std::sync::RwLock<HashMap<SharedString, MetaLoadStateWrapper<daedalus::minecraft::AssetsIndex>>>,
    java_runtime_manifests: std::sync::RwLock<HashMap<SharedString, MetaLoadStateWrapper<JavaRuntimeComponentManifest>>>,
}

pub struct MetadataManager {
    states: MetadataManagerStates,

    metadata_cache: PathBuf,
    version_manifest_cache: Arc<Path>,
    mojang_java_runtimes_cache: Arc<Path>,

    http_client: reqwest::Client,
    sender: Sender<MessageToFrontend>,
    runtime: Handle
}

pub trait MetadataItem {
    type T: DeserializeOwned + Send + Sync + 'static;

    fn url(&self) -> SharedString;
    fn state<'a>(&self, states: &'a MetadataManagerStates) -> MetaLoadStateWrapper<Self::T>;
    fn cache_file(&self, _metadata_manager: &MetadataManager) -> impl AsRef<Path> + Send + Sync + 'static;
    fn data_hash(&self) -> Option<SharedString> {
        None
    }
    fn try_send_to_backend(_value: Result<Arc<Self::T>, MetaLoadError>, _sender: Sender<MessageToFrontend>) -> impl std::future::Future<Output = ()> + Send {
        async {}
    }
}

pub struct MinecraftVersionManifestMetadata;

impl MetadataItem for MinecraftVersionManifestMetadata {
    type T = MinecraftVersionManifest;

    fn url(&self) -> SharedString {
        SharedString::new_static(MOJANG_VERSION_MANIFEST_URL)
    }

    fn cache_file(&self, metadata_manager: &MetadataManager) -> impl AsRef<Path> + Send + Sync + 'static {
        Arc::clone(&metadata_manager.version_manifest_cache)
    }

    fn state<'a>(&self, states: &'a MetadataManagerStates) -> MetaLoadStateWrapper<Self::T> {
        states.minecraft_version_manifest.clone()
    }

    fn try_send_to_backend(value: Result<Arc<Self::T>, MetaLoadError>, sender: Sender<MessageToFrontend>) -> impl std::future::Future<Output = ()> + Send {
        async move {
            let _ = sender.send(MessageToFrontend::VersionManifestUpdated(value)).await;
        }
    }
}

pub struct MojangJavaRuntimesMetadata;

impl MetadataItem for MojangJavaRuntimesMetadata {
    type T = JavaRuntimes;

    fn url(&self) -> SharedString {
        SharedString::new_static("https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json")
    }

    fn cache_file(&self, metadata_manager: &MetadataManager) -> impl AsRef<Path> + Send + Sync + 'static {
        Arc::clone(&metadata_manager.mojang_java_runtimes_cache)
    }

    fn state<'a>(&self, states: &'a MetadataManagerStates) -> MetaLoadStateWrapper<Self::T> {
        states.mojang_java_runtimes.clone()
    }
}

pub struct MinecraftVersionMetadata<'v>(pub &'v MinecraftVersionLink);

impl <'v> MetadataItem for MinecraftVersionMetadata<'v> {
    type T = MinecraftVersion;

    fn url(&self) -> SharedString {
        self.0.url.clone().into()
    }

    fn data_hash(&self) -> Option<SharedString> {
        Some(self.0.sha1.clone().into())
    }

    fn cache_file<'a>(&'a self, metadata_manager: &MetadataManager) -> impl AsRef<Path> + Send + Sync + 'static {
        if !crate::backend::is_single_component_path(&self.0.sha1) {
            panic!("Invalid sha1 {}, possible directory traversal attack?", self.0.sha1);
        }
        let mut path = metadata_manager.metadata_cache.join("version_info");
        path.push(self.0.sha1.as_str());
        path
    }

    fn state<'a>(&self, states: &'a MetadataManagerStates) -> MetaLoadStateWrapper<Self::T> {
        let url = self.url();
        if let Some(item) = states.version_info.read().unwrap().get(&url) {
            item.clone()
        } else {
            let mut write = states.version_info.write().unwrap();
            write.entry(url).or_default().clone()
        }
    }
}

pub struct AssetsIndexMetadata {
    pub url: SharedString,
    pub cache: Arc<Path>,
    pub hash: SharedString,
}

impl MetadataItem for AssetsIndexMetadata {
    type T = daedalus::minecraft::AssetsIndex;

    fn url(&self) -> SharedString {
        self.url.clone()
    }

    fn state<'a>(&self, states: &'a MetadataManagerStates) -> MetaLoadStateWrapper<Self::T> {
        let url = self.url();
        if let Some(item) = states.assets_index.read().unwrap().get(&url) {
            item.clone()
        } else {
            let mut write = states.assets_index.write().unwrap();
            write.entry(url).or_default().clone()
        }
    }

    fn cache_file(&self, _: &MetadataManager) -> impl AsRef<Path> + Send + Sync + 'static {
        Arc::clone(&self.cache)
    }
    
    fn data_hash(&self) -> Option<SharedString> {
        Some(self.hash.clone())
    }
}

pub struct MojangJavaRuntimeComponentMetadata {
    pub url: SharedString,
    pub cache: Arc<Path>,
    pub hash: SharedString,
}

impl MetadataItem for MojangJavaRuntimeComponentMetadata {
    type T = JavaRuntimeComponentManifest;

    fn url(&self) -> SharedString {
        self.url.clone()
    }

    fn state<'a>(&self, states: &'a MetadataManagerStates) -> MetaLoadStateWrapper<Self::T> {
        let url = self.url();
        if let Some(item) = states.java_runtime_manifests.read().unwrap().get(&url) {
            item.clone()
        } else {
            let mut write = states.java_runtime_manifests.write().unwrap();
            write.entry(url).or_default().clone()
        }
    }

    fn cache_file<'a>(&'a self, _: &MetadataManager) -> impl AsRef<Path> + Send + Sync + 'static {
        Arc::clone(&self.cache)
    }
    
    fn data_hash(&self) -> Option<SharedString> {
        Some(self.hash.clone())
    }
}

#[derive(thiserror::Error, Clone, Debug)]
pub enum MetaLoadError {
    InvalidHash,
    Reqwest(Arc<reqwest::Error>),
    SerdeJson(Arc<serde_json::Error>),
    TokioJoin(Arc<tokio::task::JoinError>),
}

impl Display for MetaLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidHash => {
                return f.write_str("Data did not match expected hash");
            },
            Self::Reqwest(error) => {
                if let Some(url) = error.url() {
                    if error.is_connect() {
                        return f.write_fmt(format_args!("Unable to connect to {}", url));
                    } else if error.is_timeout() {
                        return f.write_fmt(format_args!("Connection to {} timed out", url));
                    } else if error.is_decode() {
                        return f.write_fmt(format_args!("Unable to decode response from {}", url));
                    } else if error.is_builder() {
                        return f.write_fmt(format_args!("Unexpected error while constructing request to {}", url));
                    }
                } else if error.is_connect() {
                    return f.write_str("Unable to connect");
                } else if error.is_timeout() {
                    return f.write_str("Connection timed out");
                } else if error.is_decode() {
                    return f.write_str("Unable to decode response");
                } else if error.is_builder() {
                    return f.write_str("Unexpected error while constructing request");
                }

                return f.debug_tuple("Reqwest").field(error).finish();
            },
            Self::SerdeJson(_) => {
                return f.write_str("Data was missing or malformed");
            }
            Self::TokioJoin(error) => f.debug_tuple("TokioJoin").field(error).finish(),
        }
    }
}

impl From<reqwest::Error> for MetaLoadError {
    fn from(error: reqwest::Error) -> Self {
        Self::Reqwest(Arc::new(error))
    }
}

impl From<serde_json::Error> for MetaLoadError {
    fn from(error: serde_json::Error) -> Self {
        Self::SerdeJson(Arc::new(error))
    }
}

impl From<tokio::task::JoinError> for MetaLoadError {
    fn from(error: tokio::task::JoinError) -> Self {
        Self::TokioJoin(Arc::new(error))
    }
}

#[derive(Default)]
pub enum MetaLoadState<T> {
    #[default]
    Unloaded,
    Pending(JoinHandle<Result<Arc<T>, MetaLoadError>>),
    Loaded(Arc<T>),
    Error(MetaLoadError)
}

impl MetadataManager {
    pub fn new(http_client: reqwest::Client, runtime: Handle, directory: PathBuf, sender: Sender<MessageToFrontend>) -> Self {
        Self {
            states: MetadataManagerStates::default(),

            version_manifest_cache: directory.join("version_manifest.json").into(),
            mojang_java_runtimes_cache: directory.join("mojang_java_runtimes.json").into(),
            metadata_cache: directory,

            http_client,
            sender,
            runtime
        }
    }

    pub async fn load<I: MetadataItem>(&self, item: &I) {
        let state = item.state(&self.states);
        let mut state = state.lock().await;
        if matches!(*state, MetaLoadState::Unloaded) {
            let cache_file = item.cache_file(self);
            Self::inner_start_loading(&mut *state, item, Some(cache_file), &self.http_client, &self.runtime, self.sender.clone());
        }
    }

    pub async fn force_reload<I: MetadataItem>(&self, item: &I) {
        let state = item.state(&self.states);
        let mut state = state.lock().await;
        Self::inner_start_loading(&mut *state, item, None::<PathBuf>, &self.http_client, &self.runtime, self.sender.clone());
    }

    pub async fn fetch<I: MetadataItem>(&self, item: &I) -> Result<Arc<<I as MetadataItem>::T>, MetaLoadError> {
        let state = item.state(&self.states);
        let mut state = state.lock().await;
        let cache_file = item.cache_file(self);
        Self::inner_fetch(&mut *state, item, cache_file, &self.http_client, &self.runtime, self.sender.clone()).await
    }

    fn inner_start_loading<I: MetadataItem>(state: &mut MetaLoadState<I::T>, item: &I, cache_file: Option<impl AsRef<Path> + Send + Sync + 'static>, http_client: &reqwest::Client, runtime: &Handle, sender: Sender<MessageToFrontend>) {
        let url = item.url();
        let http_client = http_client.clone();
        let expected_hash = item.data_hash().and_then(|sha1| {
            let mut expected_hash = [0u8; 20];
            hex::decode_to_slice(sha1.as_str(), &mut expected_hash).ok()?;
            Some(expected_hash)
        });
        let join_handle = runtime.spawn(async move {
            let mut file_fallback = None;

            if let Some(cache_file) = &cache_file {
                if let Ok(file) = tokio::fs::read(&cache_file).await {
                    let correct_hash = if let Some(expected_hash) = &expected_hash {
                        let mut hasher = Sha1::new();
                        hasher.update(&file);
                        let actual_hash = hasher.finalize();

                        expected_hash == &*actual_hash
                    } else {
                        true
                    };

                    if correct_hash {
                        let result: Result<I::T, serde_json::Error> = serde_json::from_slice(&file);
                        match result {
                            Ok(meta) => {
                                if expected_hash.is_some() {
                                    return Ok(Arc::new(meta));
                                } else {
                                    file_fallback = Some(Arc::new(meta));
                                }
                            },
                            Err(error) => {
                                eprintln!("Error parsing cached metadata file for {:?}, downloading file again... {}", cache_file.as_ref(), error);
                            },
                        }
                    } else {
                        eprintln!("Sha1 mismatch for {:?}, downloading file again...", cache_file.as_ref());
                    }
                }
            }

            let mut result: Result<Arc<I::T>, MetaLoadError> = async move {
                let response = http_client.get(url.as_str()).send().await?;
                let bytes = response.bytes().await?;

                // We try to decode before checking the hash because it's a more
                // useful error message to know that the content is invalid
                let meta: I::T = serde_json::from_slice(&bytes)?;

                let correct_hash = if let Some(expected_hash) = &expected_hash {
                    let mut hasher = Sha1::new();
                    hasher.update(&bytes);
                    let actual_hash = hasher.finalize();

                    expected_hash == &*actual_hash
                } else {
                    true
                };

                if !correct_hash {
                    return Err(MetaLoadError::InvalidHash);
                }

                if let Some(cache_file) = &cache_file {
                    if let Some(parent) = cache_file.as_ref().parent() {
                        let _ = tokio::fs::create_dir_all(parent).await;
                    }
                    let _ = tokio::fs::write(cache_file, bytes).await;
                }

                Ok(Arc::new(meta))
            }.await;

            if let Err(error) = &result {
                if let Some(file_fallback) = file_fallback {
                    eprintln!("Error while fetching metadata, using file fallback: {error:?}");
                    result = Ok(file_fallback);
                } else {
                    eprintln!("Error while fetching metadata: {error:?}");
                }
            }

            I::try_send_to_backend(result.clone(), sender).await;

            result
        });

        *state = MetaLoadState::Pending(join_handle);
    }

     async fn inner_fetch<I: MetadataItem>(state: &mut MetaLoadState<I::T>, item: &I, cache_file: impl AsRef<Path> + Send + Sync + 'static, http_client: &reqwest::Client, runtime: &Handle, sender: Sender<MessageToFrontend>) -> Result<Arc<I::T>, MetaLoadError> {
        if let MetaLoadState::Unloaded = state {
            Self::inner_start_loading(state, item, Some(cache_file), http_client, runtime, sender);
        }

        match state {
            MetaLoadState::Unloaded => unreachable!(),
            MetaLoadState::Pending(join_handle) => {
                let result = join_handle.await.map_err(MetaLoadError::from).flatten();
                match result {
                    Ok(value) => {
                        *state = MetaLoadState::Loaded(Arc::clone(&value));
                        return Ok(value);
                    },
                    Err(error) => {
                        *state = MetaLoadState::Error(error.clone());
                        return Err(error);
                    },
                }
            },
            MetaLoadState::Loaded(value) => {
                return Ok(Arc::clone(value));
            },
            MetaLoadState::Error(meta_load_error) => {
                return Err(meta_load_error.clone())
            },
        }
    }
}