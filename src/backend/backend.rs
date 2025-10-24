use std::{collections::HashSet, sync::Arc, time::Duration};

use gpui::SharedString;
use tokio::{runtime::Runtime, sync::mpsc::{Receiver, Sender}};

use crate::{backend::{instance::Instance, launch::Launcher, metadata::{manager::{MetadataManager, MinecraftVersionManifestMetadata, MinecraftVersionMetadata, MojangJavaRuntimeComponentMetadata, MojangJavaRuntimesMetadata}, schemas::version::LaunchArgument}}, bridge::{MessageToBackend, MessageToFrontend, ProgressTrackers}};

pub fn start() -> (BackendHandle, Receiver<MessageToFrontend>) {
    let (send_to_backend, recv_from_frontend) = tokio::sync::mpsc::channel(1024);
    let (send_to_frontend, recv_from_backend) = tokio::sync::mpsc::channel(1024);

    let runtime = Arc::new(tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("Failed to initialize Tokio runtime"));

    let http_client = reqwest::ClientBuilder::new()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(30))
        .user_agent("LauncherExperiment/0.1.0")
        .build().unwrap();

    let base_dirs = directories::BaseDirs::new().unwrap();
    let data_dir = base_dirs.data_dir();
    let launcher_dir = data_dir.join("launcher_experiment");
    let metadata_dir = launcher_dir.join("metadata");

    let state = BackendState {
        recv: recv_from_frontend,
        send: send_to_frontend.clone(),
        runtime: Arc::clone(&runtime),
        http_client: http_client.clone(),
        meta: Arc::new(MetadataManager::new(http_client, runtime.handle().clone(), metadata_dir, send_to_frontend.clone())),
        instances: Vec::new(),
        launcher: Launcher::new(launcher_dir, send_to_frontend),
    };

    runtime.spawn(state.handle());

    std::mem::forget(runtime);

    (
        BackendHandle { send: send_to_backend },
        recv_from_backend
    )
}

pub struct BackendState {
    recv: Receiver<MessageToBackend>,
    send: Sender<MessageToFrontend>,
    runtime: Arc<Runtime>,
    http_client: reqwest::Client,
    meta: Arc<MetadataManager>,
    instances: Vec<Instance>,
    launcher: Launcher
}

impl BackendState {
    async fn handle(mut self) {
        println!("listening...");
        while let Some(message) = self.recv.recv().await {
            println!("Got {message:?}");
            match message {
                MessageToBackend::LoadVersionManifest { reload } => {
                    if reload {
                        self.meta.force_reload(&MinecraftVersionManifestMetadata).await;
                    } else {
                        self.meta.load(&MinecraftVersionManifestMetadata).await;
                    }
                },
                MessageToBackend::CreateInstance { name, version, loader } => {
                    self.instances.push(Instance {
                        name: name.clone(), version: version.clone(), loader
                    });
                    let _ = self.send.send(MessageToFrontend::InstanceAdded { name, version, loader }).await;
                },
                MessageToBackend::StartInstance { name, progress_trackers } => {
                    // todo: do we want to put all this in a separate task? probably, right
                    self.launch(name, progress_trackers).await;
                },
                MessageToBackend::DownloadAllMetadata => {
                    self.download_all_metadata().await;
                }
            }
        }
    }

    async fn launch(&mut self, name: SharedString, progress_trackers: ProgressTrackers) {
        let Some(instance) = self.instances.iter().find(|v| v.name == name) else {
            todo!("Send notification to frontend that instance doesn't exist");
        };

        self.launcher.launch(&self.meta, &self.http_client, self.runtime.handle(), instance, name.as_str(), progress_trackers).await
    }
    
    async fn download_all_metadata(&self) {
        let Ok(versions) = self.meta.fetch(&MinecraftVersionManifestMetadata).await else {
            panic!("Unable to get Minecraft version manifest");
        };

        // let mut rules = HashSet::new();

        for link in &versions.versions {
            let Ok(versions) = self.meta.fetch(&MinecraftVersionMetadata(link)).await else {
                panic!("Unable to get load version: {:?}", link.id);
            };
            // if let Some(arguments) = &versions.arguments {
            //     for argument in arguments.game.iter().chain(arguments.jvm.iter()) {
            //         if let LaunchArgument::Ruled(ruled) = argument {
            //             rules.insert(ruled.rules.clone());
            //         }
            //     }
            // }
            // for game_library in &versions.libraries {
            //     if let Some(da_rules) = &game_library.rules {
            //         rules.insert(da_rules.clone());
            //     }
            // }
        }

        // println!("{rules:#?}");


        let Ok(runtimes) = self.meta.fetch(&MojangJavaRuntimesMetadata).await else {
            panic!("Unable to get java runtimes manifest");
        };

        for (platform_name, platform) in &runtimes.platforms {
            for (jre_component, components) in &platform.components {
                if components.is_empty() {
                    continue;
                }

                let runtime_component_dir = self.launcher.runtime_dir.join(jre_component).join(platform_name.as_str());
                let _ = std::fs::create_dir_all(&runtime_component_dir);
                let Ok(runtime_component_dir) = runtime_component_dir.canonicalize() else {
                    panic!("Unable to create runtime component dir");
                };
                
                for runtime_component in components {
                    let Ok(manifest) = self.meta.fetch(&MojangJavaRuntimeComponentMetadata {
                        url: runtime_component.manifest.url.clone().into(),
                        cache: runtime_component_dir.join("manifest.json").into(),
                        hash: runtime_component.manifest.sha1.clone().into(),
                    }).await else {
                        panic!("Unable to get java runtime component manifest");
                    };

                    let keys: &[Arc<std::path::Path>] = &[
                        std::path::Path::new("bin/java").into(),
                        std::path::Path::new("bin/javaw.exe").into(),
                        std::path::Path::new("jre.bundle/Contents/Home/bin/java").into(),
                        std::path::Path::new("MinecraftJava.exe").into(),
                    ];

                    let mut known_executable_path = false;
                    for key in keys {
                        if manifest.files.contains_key(key) {
                            known_executable_path = true;
                            break;
                        }
                    }

                    if !known_executable_path {
                        eprintln!("Warning: {}/{} doesn't contain known java executable", jre_component, platform_name);
                    }
                }
            }
        }

        println!("Done downloading all metadata");
    }
}

#[derive(Clone)]
pub struct BackendHandle {
    send: Sender<MessageToBackend>,
}

impl BackendHandle {
    pub fn send(&self, message: MessageToBackend) {
        let _ = self.send.blocking_send(message);
    }

    pub fn is_closed(&self) -> bool {
        self.send.is_closed()
    }
}