use std::sync::{atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering}, Arc, RwLock};

use daedalus::minecraft::VersionType;
use gpui::{prelude::*, *};
use gpui_component::{
    alert::Alert, button::{Button, ButtonGroup, ButtonVariants}, checkbox::Checkbox, dropdown::{Dropdown, DropdownDelegate, DropdownItem, DropdownState, SearchableVec}, form::form_field, h_flex, input::{InputEvent, InputState, TextInput}, resizable::{h_resizable, resizable_panel, ResizableState}, sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem}, skeleton::Skeleton, table::{Column, ColumnFixed, ColumnSort, Table, TableDelegate}, v_flex, ActiveTheme as _, ContextModal, Icon, IconName, IndexPath, Root, Selectable, StyledExt
};

use crate::{backend::{metadata::schemas::version_manifest::MinecraftVersionType, BackendHandle}, bridge::MessageToBackend, frontend::{entity::{instance::InstanceEntries, version::VersionEntries, DataEntities}, instance::instance_list::InstanceList}};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loader {
    Vanilla,
    Fabric,
    Forge,
    NeoForge
}

pub struct InstancePage {
    instance_table: Entity<Table<InstanceList>>,

    versions: Entity<VersionEntries>,

    backend_handle: BackendHandle,

    // minecraft_version_dropdown: Entity<DropdownState<VersionList>>,
    // minecraft_version_dropdown_with_snapshots: Entity<DropdownState<VersionList>>,

    // selected_loader: Entity<Loader>,

    // show_snapshots: Arc<AtomicBool>,
}

impl InstancePage {
    pub fn new(data: &DataEntities, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let instance_table = InstanceList::create_table(data, window, cx);

        Self {
            instance_table,
            versions: data.versions.clone(),
            backend_handle: data.backend_handle.clone(),
            // minecraft_version_dropdown,
            // selected_loader: cx.new(|_| Loader::Vanilla),
            // launcher_meta_ptr: Default::default(),
            // show_snapshots: Default::default(),
            // dropdown_contains_snapshots: Default::default(),
        }
    }
}

impl Render for InstancePage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(Button::new("create_instance")
                .label("Create Instance")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.show_create_instance_modal(window, cx);
                }))
            )
            .child(self.instance_table.clone())
    }
}

#[derive(Default)]
pub struct VersionList {
    pub versions: Vec<SharedString>,
    pub matched_versions: Vec<SharedString>,
}

impl DropdownDelegate for VersionList {
    type Item = SharedString;

    fn items_count(&self, _section: usize) -> usize {
        self.matched_versions.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&Self::Item> {
        self.matched_versions.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        Self::Item: gpui_component::dropdown::DropdownItem<Value = V>,
        V: PartialEq {

        for (ix, item) in self.matched_versions.iter().enumerate() {
            if item.value() == value {
                return Some(IndexPath::default().row(ix));
            }
        }

        None
    }

    fn searchable(&self) -> bool {
        true
    }

    fn perform_search(&mut self, query: &str, _window: &mut Window, _: &mut App) -> Task<()> {
        let lower_query = query.to_lowercase();

        self.matched_versions = self
            .versions
            .iter()
            .filter(|item| item.to_lowercase().starts_with(&lower_query))
            .cloned()
            .collect();

        Task::ready(())
    }
}

impl InstancePage {
    pub fn show_create_instance_modal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selected_loader = Arc::new(AtomicUsize::new(0));
        let force_skeleton = Arc::new(AtomicBool::new(false));
        let loaded_versions = Arc::new(AtomicBool::new(false));
        let error_loading_versions = Arc::new(RwLock::new(None));
        let show_snapshots = Arc::new(AtomicBool::new(false));

        let minecraft_version_dropdown = cx.new(|cx| {
            DropdownState::new(VersionList::default(), None, window, cx)
        });

        let text_input_state = cx.new(|cx| {
            InputState::new(window, cx).placeholder("Unnamed Instance")
        });
        
        self.versions.read(cx).load_if_missing();

        let backend_handle = self.backend_handle.clone();

        let reload_version_dropdown = {
            let versions = self.versions.clone();
            let loaded_versions = Arc::clone(&loaded_versions);
            let show_snapshots = Arc::clone(&show_snapshots);
            let error_loading_versions= Arc::clone(&error_loading_versions);
            let minecraft_version_dropdown = minecraft_version_dropdown.clone();

            move |window: &mut Window, cx: &mut App| {
                cx.update_entity(&minecraft_version_dropdown, |dropdown, cx| {
                    let versions = versions.read(cx);

                    let (versions, latest) = if let Some(manifest) = &versions.manifest {
                        match manifest {
                            Ok(manifest) => {
                                loaded_versions.store(true, Ordering::Relaxed);
                                *error_loading_versions.write().unwrap() = None;

                                let versions: Vec<SharedString> = if show_snapshots.load(Ordering::Relaxed) {
                                    manifest.versions.iter().map(|v| v.id.clone()).collect()
                                } else {
                                    manifest.versions.iter().filter(|v| !matches!(v.r#type, MinecraftVersionType::Snapshot)).map(|v| v.id.clone()).collect()
                                };

                                (versions, Some(manifest.latest.release.clone()))
                            },
                            Err(error) => {
                                loaded_versions.store(false, Ordering::Relaxed);
                                *error_loading_versions.write().unwrap() = Some(error.clone());
                                (Vec::new(), None)
                            },
                        }
                    } else {
                        loaded_versions.store(false, Ordering::Relaxed);
                        (Vec::new(), None)
                    };

                    let mut to_select = None;

                    if let Some(last_selected) = dropdown.selected_value().cloned() {
                        if versions.contains(&last_selected) {
                            to_select = Some(last_selected);
                        }
                    }

                    if to_select.is_none() {
                        if let Some(latest) = latest {
                            if versions.contains(&latest) {
                                to_select = Some(latest);
                            }
                        }
                    }

                    if to_select.is_none() {
                        to_select = versions.first().map(SharedString::clone);
                    }

                    dropdown.set_items(VersionList {
                        versions: versions.clone(),
                        matched_versions: versions
                    }, window, cx);

                    if let Some(to_select) = to_select {
                        dropdown.set_selected_value(&to_select, window, cx);
                    }

                    cx.notify();
                });
            }
        };

        (reload_version_dropdown)(window, cx);

        let subscription = {
            let window_handle = window.window_handle();
            let reload_version_dropdown = reload_version_dropdown.clone();
            cx.observe(&self.versions, move |_, _, cx| {
                let _ = window_handle.update(cx, |_, window, cx| {
                    (reload_version_dropdown)(window, cx);
                });
            })
        };

        let versions = self.versions.clone();

        window.open_modal(cx, move |modal, window, cx| {
            let _ = &subscription;

            text_input_state.update(cx, |input_state, cx| {
                let selected = minecraft_version_dropdown.read(cx).selected_value().cloned().unwrap_or("Unnamed Instance".into());
                input_state.set_placeholder(selected, window, cx);
            });

            if let Some(error) = error_loading_versions.read().unwrap().as_ref() {
                let error_widget = Alert::new(
                    "error",
                    format!("{}", error)
                ).icon(IconName::CircleX).title("Error loading Minecraft versions");

                let versions = versions.clone();
                let error_loading_versions = Arc::clone(&error_loading_versions);
                let reload_button = Button::new("reload-versions")
                    .primary()
                    .label("Reload Versions")
                    .on_click(move |_, _, cx| {
                        *error_loading_versions.write().unwrap() = None;
                        versions.read(cx).reload();
                    });

                return modal
                    .confirm()
                    .title("Create Instance")
                    .child(v_flex().gap_3().child(error_widget).child(reload_button));
            }

            let is_force_skeleton = force_skeleton.load(Ordering::Relaxed);

            let force_skeleton = Arc::clone(&force_skeleton);

            let selected_loader_value = match selected_loader.load(Ordering::Relaxed) {
                0 => Loader::Vanilla,
                1 => Loader::Fabric,
                2 => Loader::Forge,
                3 => Loader::NeoForge,
                _ => unreachable!()
            };

            #[inline]
            fn labelled(label: &'static str, element: impl IntoElement) -> Div {
                v_flex().gap_1().child(div().text_sm().font_medium().child(label)).child(element)
            }

            let version_dropdown;
            let show_snapshots_button;
            let loader_button_group;

            if !loaded_versions.load(Ordering::Relaxed) || is_force_skeleton {
                version_dropdown = Dropdown::new(&minecraft_version_dropdown).w_full().disabled(true).placeholder("Loading Minecraft Versions...");
                show_snapshots_button = Skeleton::new().w_full().min_h_4().max_h_4().rounded_md().into_any_element();
                loader_button_group = Skeleton::new().w_full().min_h_8().max_h_8().rounded_md().into_any_element();
            } else {
                let reload_version_dropdown = reload_version_dropdown.clone();
                let selected_loader = selected_loader.clone();

                let show_snapshots = Arc::clone(&show_snapshots);
                let show_snapshots_value = show_snapshots.load(Ordering::Relaxed);
                
                version_dropdown = Dropdown::new(&minecraft_version_dropdown).title_prefix("Minecraft Version: ");
                show_snapshots_button = Checkbox::new("show_snapshots")
                                .checked(show_snapshots_value)
                                .label("Show Snapshots")
                                .on_click(move |show, window, cx| {
                                    show_snapshots.store(*show, Ordering::Relaxed);
                                    (reload_version_dropdown)(window, cx);
                                }).into_any_element();
                loader_button_group = ButtonGroup::new("loader")
                        .outline()
                        .h_full()
                        .child(
                            Button::new("loader-vanilla")
                                .label("Vanilla")
                                .selected(selected_loader_value == Loader::Vanilla),
                        )
                        .child(
                            Button::new("loader-fabric")
                                .label("Fabric")
                                .selected(selected_loader_value == Loader::Fabric),
                        )
                        .child(
                            Button::new("loader-forge")
                                .label("Forge")
                                .selected(selected_loader_value == Loader::Forge),
                        )
                        .child(
                            Button::new("loader-neoforge")
                                .label("NeoForge")
                                .selected(selected_loader_value == Loader::NeoForge),
                        )
                        .on_click(move |selected, _, _| {
                            match selected.first() {
                                Some(0) => selected_loader.store(0, Ordering::Relaxed),
                                Some(1) => selected_loader.store(1, Ordering::Relaxed),
                                Some(2) => selected_loader.store(2, Ordering::Relaxed),
                                Some(3) => selected_loader.store(3, Ordering::Relaxed),
                                _ => {}
                            };
                        }).into_any_element();
            };

            let minecraft_version_dropdown = minecraft_version_dropdown.clone();

            let content = v_flex()
                    .gap_3()
                    .child(labelled("Name", TextInput::new(&text_input_state)))
                    .child(labelled("Version", v_flex().gap_2()
                        .child(version_dropdown)
                        .child(show_snapshots_button)
                    ))
                    .child(labelled("Modloader", loader_button_group));

            let text_input_state = text_input_state.clone();
            let backend_handle = backend_handle.clone();

            modal
                .confirm()
                .title("Create Instance")
                .child(Checkbox::new("force_skeleton")
                    .label("Force Skeleton")
                    .checked(is_force_skeleton)
                    .on_click(move |v, _, _| {
                        force_skeleton.store(*v, Ordering::Relaxed);
                    })
                )
                .on_ok(move |_, _, cx| {
                    let Some(selected_version) = minecraft_version_dropdown.read(cx).selected_value().cloned() else {
                        return false;
                    };

                    let mut name = text_input_state.read(cx).value().clone();
                    if name.is_empty() {
                        name = selected_version.clone();
                    }

                    backend_handle.send(MessageToBackend::CreateInstance {
                        name,
                        version: selected_version,
                        loader: selected_loader_value
                    });

                    return true;
                })
                .child(content)
        });
    }
}