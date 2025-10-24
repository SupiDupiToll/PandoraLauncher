use std::sync::{atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering}, Arc, RwLock};

use daedalus::minecraft::VersionType;
use gpui::{prelude::*, *};
use gpui_component::{
    alert::Alert, button::{Button, ButtonGroup, ButtonVariants}, checkbox::Checkbox, dropdown::{Dropdown, DropdownDelegate, DropdownItem, DropdownState, SearchableVec}, form::form_field, h_flex, input::{InputEvent, InputState, TextInput}, resizable::{h_resizable, resizable_panel, ResizableState}, sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem}, skeleton::Skeleton, table::{Column, ColumnFixed, ColumnSort, Table, TableDelegate}, v_flex, ActiveTheme as _, ContextModal, Icon, IconName, IndexPath, Root, Selectable, StyledExt
};

use crate::{backend::{metadata::schemas::version_manifest::MinecraftVersionType, BackendHandle}, bridge::MessageToBackend, frontend::{entity::{instance::InstanceEntries, version::VersionEntries, DataEntities}, instance::instance_list::InstanceList}};

pub struct DebugPage {
    backend_handle: BackendHandle,
}

impl DebugPage {
    pub fn new(data: &DataEntities, window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            backend_handle: data.backend_handle.clone()
        }
    }
}

impl Render for DebugPage {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(Button::new("download_all_metadata")
                .label("Download All Metadata")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.backend_handle.send(MessageToBackend::DownloadAllMetadata);
                }))
            )
    }
}