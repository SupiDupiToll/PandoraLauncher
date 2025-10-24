use std::sync::Arc;

use gpui::{prelude::*, *};
use gpui_component::Root;

use crate::{bridge::MessageToFrontend, frontend::{entity::{instance::{InstanceEntries, InstanceEntry}, version::VersionEntries, DataEntities}, root::LauncherRoot}};

pub mod backend;
pub mod frontend;
pub mod bridge;
pub mod panic;

// Init translations
rust_i18n::i18n!();

actions!(
    [CreateInstance]
);

#[derive(rust_embed::RustEmbed)]
#[folder = "./assets"]
#[include = "icons/**/*.svg"]
pub struct Assets;

macro_rules! ts {
    ($($all:tt)*) => {
        gpui::SharedString::from(crate::_rust_i18n_t!($($all)*))
    }
}
pub(crate) use ts;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }

        Self::get(path)
            .map(|f| Some(f.data))
            .ok_or_else(|| anyhow::anyhow!("could not find asset at path \"{path}\""))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|p| p.starts_with(path).then(|| p.into()))
            .collect())
    }
}

fn main() {
    let panic_message = Default::default();

    crate::panic::install_hook(Arc::clone(&panic_message));

    Application::new().with_assets(Assets).run(|cx: &mut App| {
        gpui_component::init(cx);

        let bounds = Bounds::centered(None, size(px(500.), px(500.0)), cx);

        let (backend_handle, mut recv) = crate::backend::start();

        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |window, cx| {
                let instances = cx.new(|_| InstanceEntries { entries: Vec::new() });
                let versions = cx.new(|_| VersionEntries::new(backend_handle.clone()));
                let data = DataEntities {
                    instances,
                    versions,
                    backend_handle
                };

                {
                    let data = data.clone();
                    cx.spawn(async move |app| {
                        while let Some(message) = recv.recv().await {
                            match message {
                                MessageToFrontend::VersionManifestUpdated(result) => {
                                    VersionEntries::set(&data.versions, result, app);
                                },
                                MessageToFrontend::InstanceAdded { name, version, loader } => {
                                    InstanceEntries::add(&data.instances, InstanceEntry {
                                        name,
                                        version,
                                    }, app);
                                },
                                MessageToFrontend::Refresh => {
                                    let _ = app.refresh();
                                }
                            }
                        }
                    }).detach();
                }

                let launcher_root = cx.new(|cx| LauncherRoot::new(&data, panic_message, window, cx));
                cx.new(|cx| Root::new(launcher_root.into(), window, cx))
            },
        ).unwrap();
    
        cx.activate(true);
    });
}
