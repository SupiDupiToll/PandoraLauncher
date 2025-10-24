use std::{borrow::Cow, sync::{Arc, RwLock}};

use gpui::{prelude::*, *};
use gpui_component::{
    button::Button, dropdown::Dropdown, h_flex, input::{InputEvent, InputState, TextInput}, resizable::{h_resizable, resizable_panel, ResizableState}, sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem}, table::{Column, ColumnFixed, ColumnSort, Table, TableDelegate}, v_flex, ActiveTheme as _, ContextModal, Icon, IconName, Root
};
use rand::RngCore;
use rust_embed::Embed;

use crate::{backend::BackendHandle, frontend::{entity::{instance::InstanceEntries, version::VersionEntries, DataEntities}, ui::LauncherUI}};

pub struct LauncherRoot {
    pub ui: Entity<LauncherUI>,
    pub panic_message: Arc<RwLock<Option<String>>>,
    pub backend_handle: BackendHandle,
}

impl LauncherRoot {
    pub fn new(data: &DataEntities, panic_message: Arc<RwLock<Option<String>>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let launcher_ui = cx.new(|cx| LauncherUI::new(&data, window, cx));

        Self {
            ui: launcher_ui,
            panic_message,
            backend_handle: data.backend_handle.clone()
        }
    }
}

impl Render for LauncherRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let drawer_layer = Root::render_drawer_layer(window, cx);
        let modal_layer = Root::render_modal_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        if let Some(message) = &*self.panic_message.read().unwrap() {
            return v_flex().size_full().bg(gpui::blue()).child(message.clone());
        }
        if self.backend_handle.is_closed() {
            return v_flex().size_full().bg(gpui::red()).child("Backend has abruptly shutdown");
        }

        div()
            .size_full()
            .child(
                v_flex()
                    .size_full()
                    .child(div().flex_1().overflow_hidden().child(self.ui.clone())),
            )
            .children(drawer_layer)
            .children(modal_layer)
            .children(notification_layer)
    }
}