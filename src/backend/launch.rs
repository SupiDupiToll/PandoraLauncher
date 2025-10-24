use std::{borrow::Cow, cell::OnceCell, collections::{HashMap, HashSet}, ffi::{OsStr, OsString}, path::{Path, PathBuf}, process::Command, sync::{atomic::AtomicBool, Arc, OnceLock}, time::{Duration, Instant}};

use futures::{FutureExt, TryFutureExt};
use gpui::SharedString;
use lzma::LzmaError;
use regex::Regex;
use sha1::{Digest, Sha1};
use tokio::runtime::Handle;

use crate::{backend::{instance::Instance, metadata::{manager::{AssetsIndexMetadata, MetaLoadError, MetadataManager, MinecraftVersionManifestMetadata, MinecraftVersionMetadata, MojangJavaRuntimeComponentMetadata, MojangJavaRuntimesMetadata}, schemas::{java_runtime_component::{JavaRuntimeComponentFile, JavaRuntimeComponentManifest}, version::{LaunchArgument, LaunchArguments, MinecraftVersion, OsName, Rule, RuleAction}}}}, bridge::{MessageToFrontend, ProgressTracker, ProgressTrackers}};

use super::metadata::schemas::version::LaunchArgumentValue;

pub struct Launcher {
    assets_index_dir: PathBuf,
    assets_objects_dir: PathBuf,
    temp_natives_base_dir: PathBuf,
    pub runtime_dir: PathBuf,
    sender: tokio::sync::mpsc::Sender<MessageToFrontend>
}

pub enum LaunchGameError {
    InvalidAssetHash
}

impl Launcher {
    pub fn new(launcher_dir: PathBuf, sender: tokio::sync::mpsc::Sender<MessageToFrontend>) -> Self {
        let assets_dir = launcher_dir.join("assets");
        let assets_index_dir = assets_dir.join("indexes");
        let assets_objects_dir = assets_dir.join("objects");

        let runtime_dir = launcher_dir.join("runtime");

        let temp_dir = launcher_dir.join("temp");
        let temp_natives_base_dir = temp_dir.join("natives");

         Self {
            assets_index_dir,
            assets_objects_dir,
            temp_natives_base_dir,
            runtime_dir,
            sender
         }
    }

    pub async fn launch(&self, meta: &Arc<MetadataManager>, http_client: &reqwest::Client, handle: &Handle, instance: &Instance, instance_name: &str, progress_trackers: ProgressTrackers) {
        let launch_tracker = ProgressTracker::new(SharedString::new_static("Launching"), self.sender.clone());
        progress_trackers.push(launch_tracker.clone());

        launch_tracker.set_total(7);
        
        let Ok(versions) = meta.fetch(&MinecraftVersionManifestMetadata).await else {
            todo!("Send notification about versions failing to fetch");
        };

        launch_tracker.add_count(1);
        launch_tracker.notify().await;

        let Some(version) = versions.versions.iter().find(|v| v.id == instance.version) else {
            todo!("Version doesn't exist");
        };

        let Ok(version_info) = meta.fetch(&MinecraftVersionMetadata(version)).await else {
            todo!("Can't get version info");
        };

        launch_tracker.add_count(1);
        launch_tracker.notify().await;

        if !crate::backend::is_single_component_path(instance_name) {
            // todo: add filtering to ensure that the path is valid (eg. replace .. with _)
            panic!("Invalid path");
        }

        let mojang_java_binary_future = 
            self.load_mojang_java_binary(meta, &http_client, &version_info, &progress_trackers);
        let load_assets_future = 
            self.load_assets(meta, &http_client, &version_info, &progress_trackers);

        let result = futures::future::try_join(
            mojang_java_binary_future.map_err(LaunchError::from),
            load_assets_future.map_err(LaunchError::from)
        ).await;

        let Ok((java_path, _)) = result else {
            todo!("handle error: {:?}", result.unwrap_err());
        };

        // Compute natives path based on combined hash of all libraries
        let natives_dir = self.temp_natives_base_dir.join(calculate_natives_dirname(&version_info));
        let _ = std::fs::create_dir_all(&natives_dir);
        
        let launch_context = LaunchContext {
            java_path,
            natives_dir,
            is_demo_user: false,
            custom_resolution: None,
            quick_play: None,
        };

        let mut command = launch_context.build(&version_info);
        command.spawn().unwrap();

        launch_tracker.add_count(1);
        launch_tracker.notify().await;

        // dbg!(runtimes);
    }

    async fn load_mojang_java_binary(&self, meta: &MetadataManager, http_client: &reqwest::Client, version_info: &MinecraftVersion, progress_trackers: &ProgressTrackers) -> Result<PathBuf, LoadJavaRuntimeError> {
        let platform = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("linux", "x86_64") => SharedString::new_static("linux"),
            ("linux", "x86") => SharedString::new_static("linux-i386"),
            ("macos", "x86_64") => SharedString::new_static("mac-os"),
            ("macos", "aarch64") => SharedString::new_static("mac-os-arm64"),
            ("windows", "aarch64") => SharedString::new_static("windows-arm64"),
            ("windows", "x86_64") => SharedString::new_static("windows-x64"),
            ("windows", "x86") => SharedString::new_static("windows-x86"),
            ("macos", b) => format!("mac-os-{b}").into(),
            (a, b) => format!("{a}-{b}").into(),
        };

        let jre_component = if let Some(java_version) = &version_info.java_version {
            java_version.component.as_str()
        } else {
            "jre-legacy"
        };

        if !crate::backend::is_single_component_path(jre_component) {
            return Err(LoadJavaRuntimeError::InvalidComponentPath);
        }
        if !crate::backend::is_single_component_path(&platform) {
            return Err(LoadJavaRuntimeError::InvalidComponentPath);
        }

        let runtime_component_dir = self.runtime_dir.join(jre_component).join(platform.as_str());
        let _ = std::fs::create_dir_all(&runtime_component_dir);
        let Ok(runtime_component_dir) = runtime_component_dir.canonicalize() else {
            return Err(LoadJavaRuntimeError::InvalidComponentPath);
        };

        let fresh_install = !runtime_component_dir.exists();

        let runtimes = meta.fetch(&MojangJavaRuntimesMetadata).await?;

        let runtime_platform = runtimes.platforms.get(platform.as_str()).ok_or(LoadJavaRuntimeError::UnknownPlatform)?;
        let runtime_components = runtime_platform.components.get(jre_component).ok_or(LoadJavaRuntimeError::UnknownComponentForPlatform)?;
        let runtime_component = runtime_components.first().ok_or(LoadJavaRuntimeError::UnknownComponentForPlatform)?;

        let runtime = meta.fetch(&MojangJavaRuntimeComponentMetadata {
            url: runtime_component.manifest.url.clone().into(),
            cache: runtime_component_dir.join("manifest.json").into(),
            hash: runtime_component.manifest.sha1.clone().into(),
        }).await?;

        let initial_title = if fresh_install {
            SharedString::new_static("Downloading Java Runtime")
        } else {
            SharedString::new_static("Verifying integrity of Java Runtime")
        };

        let java_runtime_tracker = ProgressTracker::new(initial_title, self.sender.clone());
        progress_trackers.trackers.write().unwrap().push(java_runtime_tracker.clone());

        let result = do_java_runtime_load(&http_client, runtime_component_dir, fresh_install, runtime, &java_runtime_tracker).await;

        java_runtime_tracker.set_finished(result.is_err());
        java_runtime_tracker.notify().await;

        result
    }

    async fn load_assets(&self, meta: &MetadataManager, http_client: &reqwest::Client, version_info: &MinecraftVersion, progress_trackers: &ProgressTrackers) -> Result<(), LoadAssetObjectsError> {
        let asset_index = format!("{}-{}", version_info.id, version_info.assets);

        let Ok(assets_index) = meta.fetch(&AssetsIndexMetadata {
            url: version_info.asset_index.url.clone().into(),
            cache: self.assets_index_dir.join(asset_index).into(),
            hash: version_info.asset_index.sha1.clone().into(),
        }).await else {
            todo!("Can't get assets index");
        };

        let initial_title = SharedString::new_static("Verifying integrity of game assets");
        let assets_tracker = ProgressTracker::new(initial_title, self.sender.clone());
        progress_trackers.push(assets_tracker.clone());

        let result = do_asset_objects_load(&http_client, assets_index, self.assets_objects_dir.clone(), &assets_tracker).await;

        assets_tracker.set_finished(result.is_err());
        assets_tracker.notify().await;

        result
    }
}

fn calculate_natives_dirname(version_info: &Arc<MinecraftVersion>) -> String {
    let mut hashes = HashSet::new();
    for library in &version_info.libraries {
        if let Some(artifact) = &library.downloads.artifact {
            let mut hash = [0_u8; 20];
            if let Ok(_) = hex::decode_to_slice(artifact.sha1.as_str(), &mut hash) {
                hashes.insert(hash);
            }
        }
        if let Some(classifiers) = &library.downloads.classifiers {
            for (_, artifact) in classifiers {
                let mut hash = [0_u8; 20];
                if let Ok(_) = hex::decode_to_slice(artifact.sha1.as_str(), &mut hash) {
                    hashes.insert(hash);
                }
            }
        }
    }
    let mut combined = [0_u8; 20];
    for hash in hashes {
        for i in 0..20 {
            combined[i] ^= hash[i];
        }
    }
    hex::encode(combined)
}

#[derive(thiserror::Error, Debug)]
pub enum LaunchError {
    #[error("failed to load java runtime")]
    LoadJavaRuntimeError(#[from] LoadJavaRuntimeError),
    #[error("failed to load game assets")]
    LoadAssetObjectsError(#[from] LoadAssetObjectsError)
}

#[derive(thiserror::Error, Debug)]
pub enum LoadJavaRuntimeError {
    #[error("failed to load remote content")]
    Reqwest(#[from] reqwest::Error),
    #[error("failed to perform I/O operation")]
    IoError(#[from] std::io::Error),
    #[error("failed to load metadata")]
    MetaLoadError(#[from] MetaLoadError),
    #[error("hash isn't a valid sha1 hash")]
    InvalidHash,
    #[error("unknown platform")]
    UnknownPlatform,
    #[error("unknown component for platform")]
    UnknownComponentForPlatform,
    #[error("mojang runtime path is invalid")]
    InvalidComponentPath,
    #[error("downloaded file had wrong response size")]
    WrongResponseSize,
    #[error("downloaded file had wrong raw size")]
    WrongRawSize,
    #[error("failed to decompress file")]
    Lzma(#[from] LzmaError),
    #[error("downloaded file had had wrong hash")]
    WrongHash,
    #[error("unable to find binary")]
    UnableToFindBinary,
}

async fn do_java_runtime_load(http_client: &reqwest::Client, runtime_component_dir: PathBuf, fresh_install: bool, runtime: Arc<JavaRuntimeComponentManifest>, java_runtime_tracker: &ProgressTracker) -> Result<PathBuf, LoadJavaRuntimeError> {
    let mut links = HashMap::new();

    // Limit max concurrent connections to 8 to avoid ratelimiting issues
    let download_semaphore = tokio::sync::Semaphore::new(8);
    let started_downloading = AtomicBool::new(fresh_install);

    let mut tasks = Vec::new();

    let mut total_size = 0;

    for (filename, contents) in &runtime.files {
        if !path_is_normal(filename) {
            continue;
        }

        let path = runtime_component_dir.join(filename);

        match contents {
            JavaRuntimeComponentFile::Directory => {
                let _ = std::fs::create_dir(path);
            },
            JavaRuntimeComponentFile::File { executable, downloads } => {
                let mut expected_hash = [0u8; 20];
                let Ok(_) = hex::decode_to_slice(downloads.raw.sha1.as_str(), &mut expected_hash) else {
                    return Err(LoadJavaRuntimeError::InvalidHash);
                };

                total_size += downloads.raw.size;

                let started_downloading = &started_downloading;
                let download_semaphore = &download_semaphore;
            
                let task = async move {
                    if let Ok(file) = tokio::fs::read(&path).await {
                        let mut hasher = Sha1::new();
                        hasher.update(&file);
                        let actual_hash = hasher.finalize();

                        if expected_hash == *actual_hash {
                            java_runtime_tracker.add_count(downloads.raw.size);
                            java_runtime_tracker.notify().await;
                            return Ok(());
                        }
                    }

                    if started_downloading.swap(true, std::sync::atomic::Ordering::Relaxed) == false {
                        java_runtime_tracker.set_title(SharedString::new_static("Downloading Java Runtime"));
                    }

                    let (lzma, size, download) = if let Some(lzma) = &downloads.lzma {
                        (true, lzma.size as usize, lzma)
                    } else {
                        (false, downloads.raw.size as usize, &downloads.raw)
                    };

                    let permit = download_semaphore.acquire().await.unwrap();
                    let response = http_client.get(download.url.as_str()).send().await?;
                    let bytes = response.bytes().await?;
                    drop(permit);

                    if bytes.len() != size {
                        return Err(LoadJavaRuntimeError::WrongResponseSize);
                    }

                    let decompressed_or_raw = if lzma {
                        let result = tokio::task::spawn_blocking(move || {
                            lzma::decompress(&*bytes)
                        }).await.unwrap();

                        match result {
                            Ok(decompressed) => Ok(decompressed),
                            Err(lzma_error) => {
                                return Err(LoadJavaRuntimeError::Lzma(lzma_error));
                            },
                        }
                    } else {
                        Err(bytes)
                    };

                    let bytes = match &decompressed_or_raw {
                        Ok(vec) => vec.as_slice(),
                        Err(bytes) => &*bytes,
                    };

                    if bytes.len() != downloads.raw.size as usize {
                        return Err(LoadJavaRuntimeError::WrongRawSize);
                    }

                    let mut hasher = Sha1::new();
                    hasher.update(&bytes);
                    let actual_hash = hasher.finalize();

                    if expected_hash != *actual_hash {
                        return Err(LoadJavaRuntimeError::WrongHash);
                    }

                    tokio::fs::write(&path, bytes).await?;

                    if cfg!(unix) && *executable {
                        use std::os::unix::fs::PermissionsExt;
                        let _ = tokio::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).await;
                    }

                    java_runtime_tracker.add_count(downloads.raw.size);
                    java_runtime_tracker.notify().await;
                    return Ok(());
                };
                tasks.push(task);
            },
            JavaRuntimeComponentFile::Link { target } => {
                links.insert(path, target.clone());
            },
        }
    }
    java_runtime_tracker.set_total(total_size);
    java_runtime_tracker.notify().await;

    futures::future::try_join_all(tasks).await?;

    if cfg!(unix) {
        for (path, target) in links {
            if let Some(parent) = path.parent() {
                if let Ok(absolute_target) = parent.join(target).canonicalize() {
                    if absolute_target.starts_with(&runtime_component_dir) {
                        let _ = std::os::unix::fs::symlink(absolute_target, path);
                    }
                }
            }
        }
    }

    let bin_java = runtime_component_dir.join("bin/java");
    if let Ok(bin_java) = bin_java.canonicalize() {
        return Ok(bin_java);
    }

    let bin_javaw = runtime_component_dir.join("bin/javaw.exe");
    if let Ok(bin_javaw) = bin_javaw.canonicalize() {
        return Ok(bin_javaw);
    }

    let jre_bundle_path = runtime_component_dir.join("jre.bundle/Contents/Home/bin/java");
    if let Ok(jre_bundle_path) = jre_bundle_path.canonicalize() {
        return Ok(jre_bundle_path);
    }

    let legacy_exe = runtime_component_dir.join("MinecraftJava.exe");
    if let Ok(legacy_exe) = legacy_exe.canonicalize() {
        return Ok(legacy_exe);
    }

    Err(LoadJavaRuntimeError::UnableToFindBinary)
}

#[derive(thiserror::Error, Debug)]
pub enum LoadAssetObjectsError {
    #[error("failed to load remote content")]
    Reqwest(#[from] reqwest::Error),
    #[error("failed to perform I/O operation")]
    IoError(#[from] std::io::Error),
    #[error("hash isn't a valid sha1 hash")]
    InvalidHash,
    #[error("downloaded file had wrong response size")]
    WrongResponseSize,
    #[error("downloaded file had had wrong hash")]
    WrongHash,
}

async fn do_asset_objects_load(http_client: &reqwest::Client, assets_index: Arc<daedalus::minecraft::AssetsIndex>, assets_objects_dir: PathBuf, assets_tracker: &ProgressTracker) -> Result<(), LoadAssetObjectsError> {
    // Limit max concurrent connections to 8 to avoid ratelimiting issues
    let download_semaphore = tokio::sync::Semaphore::new(8);
    let started_downloading = AtomicBool::new(false);

    let mut total_size = 0;

    let mut tasks = Vec::new();

    let _ = std::fs::create_dir_all(&assets_objects_dir);

    for (_, asset) in &assets_index.objects {
        let mut expected_hash = [0u8; 20];
        let Ok(_) = hex::decode_to_slice(&*asset.hash, &mut expected_hash) else {
            return Err(LoadAssetObjectsError::InvalidHash);
        };

        let mut path = assets_objects_dir.join(&asset.hash[..2]);
        let _ = std::fs::create_dir(&path);
        path.push(&asset.hash);

        total_size += asset.size;

        let started_downloading = &started_downloading;
        let download_semaphore = &download_semaphore;
    
        let url = format!("https://resources.download.minecraft.net/{}/{}", &asset.hash[..2], &asset.hash);

        let task = async move {
            if let Ok(file) = tokio::fs::read(&path).await {
                let mut hasher = Sha1::new();
                hasher.update(&file);
                let actual_hash = hasher.finalize();

                if expected_hash == *actual_hash {
                    assets_tracker.add_count(asset.size);
                    assets_tracker.notify().await;
                    return Ok(());
                }
            }

            if started_downloading.swap(true, std::sync::atomic::Ordering::Relaxed) == false {
                assets_tracker.set_title(SharedString::new_static("Downloading game assets"));
            }

            let permit = download_semaphore.acquire().await.unwrap();
            let response = http_client.get(&url).send().await?;
            let bytes = response.bytes().await?;
            drop(permit);

            if bytes.len() != asset.size as usize {
                return Err(LoadAssetObjectsError::WrongResponseSize);
            }

            let mut hasher = Sha1::new();
            hasher.update(&bytes);
            let actual_hash = hasher.finalize();

            if expected_hash != *actual_hash {
                return Err(LoadAssetObjectsError::WrongHash);
            }

            tokio::fs::write(path.clone(), bytes).await?;
            assets_tracker.add_count(asset.size);
            assets_tracker.notify().await;
            return Ok(());
        };
        tasks.push(task);
    }

    assets_tracker.set_total(total_size);
    assets_tracker.notify().await;

    if let Err(error) = futures::future::try_join_all(tasks).await {
        return Err(error);
    }

    Ok(())
}


#[derive(Clone, PartialEq, Eq)]
pub enum QuickPlayLaunch {
    Singleplayer,
    Multiplayer,
    Realms,
}

pub enum ArgumentExpansionKey {
    NativesDirectory,
    LauncherName,
    LauncherVersion,
    Classpath,
    AuthPlayerName,
    VersionName,
    GameDirectory,
    AssetsRoot,
    AssetsIndexName,
    AuthUuid,
    AuthAccessToken,
    Clientid,
    AuthXuid,
    VersionType,
    QuickPlayPath,
}

impl ArgumentExpansionKey {
    pub fn from_str(string: &str) -> Option<Self> {
        match string {
            "natives_directory" => Some(Self::NativesDirectory),
            "launcher_name" => Some(Self::LauncherName),
            "launcher_version" => Some(Self::LauncherVersion),
            "classpath" => Some(Self::Classpath),
            "auth_player_name" => Some(Self::AuthPlayerName),
            "version_name" => Some(Self::VersionName),
            "game_directory" => Some(Self::GameDirectory),
            "assets_root" => Some(Self::AssetsRoot),
            "assets_index_name" => Some(Self::AssetsIndexName),
            "auth_uuid" => Some(Self::AuthUuid),
            "auth_access_token" => Some(Self::AuthAccessToken),
            "clientid" => Some(Self::Clientid),
            "auth_xuid" => Some(Self::AuthXuid),
            "version_type" => Some(Self::VersionType),
            "quickPlayPath" => Some(Self::QuickPlayPath),
            _ => None
        }
    }
}

pub struct LaunchContext {
    pub java_path: PathBuf,
    pub natives_dir: PathBuf,
    pub is_demo_user: bool,
    pub custom_resolution: Option<(u32, u32)>,
    pub quick_play: Option<QuickPlayLaunch>,
}

impl LaunchContext {
    pub fn build(self, version_info: &MinecraftVersion) -> Command {
        let mut command = Command::new(&self.java_path);

        if let Some(arguments) = &version_info.arguments {
            self.add_arguments(&mut command, &arguments.jvm);
            self.add_arguments(&mut command, &arguments.game);
        }

        command
    }

    fn add_arguments(&self, command: &mut Command, arguments: &[LaunchArgument]) {
        for argument in arguments {
            match argument {
                LaunchArgument::Single(value) => {
                    self.add_argument(command, value);
                },
                LaunchArgument::Ruled(ruled) => {
                    if self.check_rules(&ruled.rules) {
                        self.add_argument(command, &ruled.value);
                    }
                },
            }
        }
    }

    fn add_argument(&self, command: &mut Command, value: &LaunchArgumentValue) {
        match value {
            LaunchArgumentValue::Single(string) => {
                command.arg(&*self.expand_argument(string));
            },
            LaunchArgumentValue::Multiple(strings) => {
                for string in strings.iter() {
                    command.arg(&*self.expand_argument(string));
                }
            },
        }
    }

    fn expand_argument<'a>(&self, argument: &'a str) -> Cow<'a, OsStr> {
        let mut dollar_last = false;
        let mut builder = OsString::new();
        let mut copied_to_builder = 0;
        for (i, character) in argument.char_indices() {
            if character == '$' {
                dollar_last = true;
            } else if dollar_last && character == '{' {
                let remaining = &argument[i..];
                if let Some(end) = remaining.find('}') {
                    let to_expand = &argument[i+1..i+end];
                    if let Some(to_expand) = ArgumentExpansionKey::from_str(to_expand) {
                        let expanded = self.resolve_expansion(to_expand);
                        builder.push(&argument[copied_to_builder..i-1]);
                        builder.push(expanded);
                        copied_to_builder = i+end+1;
                    } else {
                        panic!("Unsupported argument: {:?}", to_expand);
                    }
                }
            } else {
                dollar_last = false;
            }
        }
        if !builder.is_empty() {
            builder.push(&argument[copied_to_builder..]);
            dbg!(argument);
            return Cow::Owned(builder);
        }
        Cow::Borrowed(OsStr::new(argument))
    }

    fn resolve_expansion(&self, key: ArgumentExpansionKey) -> &OsStr {
        match key {
            ArgumentExpansionKey::NativesDirectory => self.natives_dir.as_os_str(),
            ArgumentExpansionKey::LauncherName => OsStr::new("LauncherExperiment"),
            ArgumentExpansionKey::LauncherVersion => OsStr::new("1.0.0"),
            ArgumentExpansionKey::Classpath => todo!(),
            ArgumentExpansionKey::AuthPlayerName => todo!(),
            ArgumentExpansionKey::VersionName => todo!(),
            ArgumentExpansionKey::GameDirectory => todo!(),
            ArgumentExpansionKey::AssetsRoot => todo!(),
            ArgumentExpansionKey::AssetsIndexName => todo!(),
            ArgumentExpansionKey::AuthUuid => todo!(),
            ArgumentExpansionKey::AuthAccessToken => todo!(),
            ArgumentExpansionKey::Clientid => todo!(),
            ArgumentExpansionKey::AuthXuid => todo!(),
            ArgumentExpansionKey::VersionType => todo!(),
            ArgumentExpansionKey::QuickPlayPath => todo!(),
        }
    }

    pub fn check_rules(&self, rules: &[Rule]) -> bool {
        if cfg!(debug_assertions) && rules.is_empty() {
            panic!("Don't know what expected behaviour of empty ruleset is");
        }

        let mut allowed = false;
        for rule in rules {
            if self.check_rule(rule) {
                allowed = match rule.action {
                    RuleAction::Allow => true,
                    RuleAction::Disallow => false,
                };
            }
        }
        allowed
    }

    pub fn check_rule(&self, rule: &Rule) -> bool {
        if cfg!(debug_assertions) && rule.features.is_some() == rule.os.is_some() {
            panic!("Expected either features or os, not both/neither, {rule:?}");
        }

        if let Some(features) = &rule.features {
            if cfg!(debug_assertions) && features.is_demo_user as u8 + features.has_custom_resolution as u8 + features.has_quick_plays_support as u8 +
                    features.is_quick_play_singleplayer as u8 + features.is_quick_play_multiplayer as u8 + features.is_quick_play_realms as u8 != 1 {
                panic!("Expected exactly one feature, {rule:?}");
            }

            if features.is_demo_user {
                return self.is_demo_user;
            }
            if features.has_custom_resolution {
                return self.custom_resolution.is_some();
            }
            if features.has_quick_plays_support {
                return true;
            }
            if features.is_quick_play_singleplayer {
                return self.quick_play == Some(QuickPlayLaunch::Singleplayer);
            }
            if features.is_quick_play_multiplayer {
                return self.quick_play == Some(QuickPlayLaunch::Multiplayer);
            }
            if features.is_quick_play_realms {
                return self.quick_play == Some(QuickPlayLaunch::Realms);
            }
        }

        if let Some(os) = &rule.os {
            if cfg!(debug_assertions) && os.name.is_none() && os.arch.is_none() {
                panic!("Expected either os.name or os.arch to be present, {rule:?}");
            }

            if let Some(name) = &os.name {
                let matches = match name {
                    OsName::Linux => std::env::consts::OS == "linux",
                    OsName::Osx => std::env::consts::OS == "macos",
                    OsName::Windows => std::env::consts::OS == "windows",
                };
                if !matches {
                    return false;
                }
            }
            if let Some(arch) = &os.arch {
                match arch {
                    crate::backend::metadata::schemas::version::OsArch::Arm64 => {
                        if std::env::consts::ARCH != "aarch64" {
                            return false;
                        }

                    },
                    crate::backend::metadata::schemas::version::OsArch::X86 => {
                        if std::env::consts::ARCH != "x86" {
                            return false;
                        }
                    },
                }
            }
            if let Some(version) = &os.version {
                if let Ok(regex) = Regex::new(version.as_str()) {
                    static OS_VERSION: OnceLock<String> = OnceLock::new();
                    let os_version = OS_VERSION.get_or_init(|| format!("{}", os_info::get().version()));
                    if !regex.is_match(&os_version) {
                        return false;
                    }
                }
            }

            return true;
        }


        if cfg!(debug_assertions) {
            panic!("Unable to match rule, {rule:?}");
        }

        false
    }
}

fn path_is_normal(path: impl AsRef<Path>) -> bool {
    let components = path.as_ref().components();

    for component in components {
        match component {
            std::path::Component::Prefix(_) => return false,
            std::path::Component::RootDir => return false,
            std::path::Component::CurDir => return false,
            std::path::Component::ParentDir => return false,
            std::path::Component::Normal(_) => {},
        }
    }

    true
}