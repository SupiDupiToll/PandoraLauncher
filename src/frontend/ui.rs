use gpui::{prelude::*, *};
use gpui_component::{
    button::Button, dropdown::Dropdown, h_flex, indicator::Indicator, input::{InputEvent, InputState, TextInput}, resizable::{h_resizable, resizable_panel, ResizableState}, sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem}, table::{Column, ColumnFixed, ColumnSort, Table, TableDelegate}, v_flex, ActiveTheme as _, ContextModal, Icon, IconName, Root
};
use rand::RngCore;
use serde::Deserialize;

use crate::frontend::{entity::{instance::InstanceEntries, DataEntities}, instance::instance_page::InstancePage, pages::debug_page::DebugPage};

pub struct LauncherUI {
    data: DataEntities,
    page: LauncherPage,
    sidebar_state: Entity<ResizableState>,
}

#[derive(Clone)]
pub enum LauncherPage {
    Instances(Entity<InstancePage>),
    Debug(Entity<DebugPage>)
}

impl LauncherPage {
    pub fn into_any_element(self) -> AnyElement {
        match self {
            LauncherPage::Instances(entity) => entity.into_any_element(),
            LauncherPage::Debug(entity) => entity.into_any_element(),
        }
    }
}

impl LauncherUI {
    pub fn new(data: &DataEntities, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let instance_page = cx.new(|cx| InstancePage::new(data, window, cx));
        let sidebar_state = ResizableState::new(cx);
        
        Self {
            data: data.clone(),
            page: LauncherPage::Instances(instance_page),
            sidebar_state,
        }
    }

    fn switch_page(&mut self, page: PageType, window: &mut Window, cx: &mut Context<Self>) {
        let data = &self.data;
        match page {
            PageType::Instances => {
                if let LauncherPage::Instances(_) = self.page {
                    return;
                }
                self.page = LauncherPage::Instances(cx.new(|cx| InstancePage::new(data, window, cx)));
                cx.notify();
            },
            PageType::Debug => {
                if let LauncherPage::Debug(_) = self.page {
                    return;
                }
                self.page = LauncherPage::Debug(cx.new(|cx| DebugPage::new(data, window, cx)));
                cx.notify();
            },
        }
    }
}

#[derive(Copy, Clone)]
pub enum PageType {
    Instances,
    Debug
}

impl Render for LauncherUI {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let sidebar = Sidebar::left()
            .width(relative(1.))
            .border_width(px(0.))
            .footer(
                v_flex()
                    .w_full()
                    .gap_4()
                    .child(
                        SidebarFooter::new()
                            .w_full()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(cx.theme().radius)
                                    .bg(cx.theme().primary)
                                    .text_color(cx.theme().primary_foreground)
                                    .size_8()
                                    .flex_shrink_0()
                                    .child(Icon::new(
                                        IconName::GalleryVerticalEnd,
                                    ))
                                    .rounded_lg(),
                            )
                            .child(
                                v_flex()
                                    .gap_0()
                                    .text_sm()
                                    .flex_1()
                                    .line_height(relative(1.25))
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .child("Moulberry"),
                            )
                    ))
            .children([SidebarGroup::new("Library").child(SidebarMenu::new().children([
                SidebarMenuItem::new("Instances").on_click(cx.listener(|launcher, _, window, cx| {
                    launcher.switch_page(PageType::Instances, window, cx);
                })),
                SidebarMenuItem::new("Mods"),
                SidebarMenuItem::new("Worlds"),
            ])),
            SidebarGroup::new("Launcher").child(SidebarMenu::new().children([
                SidebarMenuItem::new("Debug").on_click(cx.listener(|launcher, _, window, cx| {
                    launcher.switch_page(PageType::Debug, window, cx);
                })),
                SidebarMenuItem::new("Blah 2"),
                SidebarMenuItem::new("Blah 3"),
            ]))]);
            // .children(stories.clone().into_iter().enumerate().map(
            //     |(group_ix, (group_name, sub_stories))| {
            //         SidebarGroup::new(*group_name).child(
            //             SidebarMenu::new().children(
            //                 sub_stories.iter().enumerate().map(|(ix, story)| {
            //                     SidebarMenuItem::new(story.read(cx).name.clone())
            //                         .active(
            //                             self.active_group_index == Some(group_ix)
            //                                 && self.active_index == Some(ix),
            //                         )
            //                         .on_click(cx.listener(
            //                             move |this, _: &ClickEvent, _, cx| {
            //                                 this.active_group_index =
            //                                     Some(group_ix);
            //                                 this.active_index = Some(ix);
            //                                 cx.notify();
            //                             },
            //                         ))
            //                 }),
            //             ),
            //         )
            //     },
            // )),

        h_resizable("container", self.sidebar_state.clone())
            .child(
                resizable_panel()
                    .size(px(150.))
                    .size_range(px(100.)..px(200.))
                    .child(sidebar),
            )
            .child(
                v_flex()
                    .flex_1()
                    .h_full()
                    .overflow_x_hidden()
                    .child(
                        h_flex()
                            .id("header")
                            .p_4()
                            .border_b_1()
                            .border_color(cx.theme().border)
                            .text_xl()
                            .child("Instances"),
                    )
                    .child(
                        div()
                            .id("page")
                            .flex_1()
                            .overflow_y_scroll()
                            .child(self.page.clone().into_any_element())
                    )
                    .into_any_element(),
            )
    }
}

// pub fn start_launching(window: &mut Window, cx: &mut App, name: &str) {
//     let instance = with_launcher_context_mut(cx, |context, _| {
//         context.instances.iter().find(|i| i.name == name).cloned()
//     });

//     let Some(instance) = instance else {
//         let name = name.to_string();
//         window.open_modal(cx, move |modal, _, _| {
//             modal.title(format!("Unable to find instance with name '{}'", name))
//                 .footer(|_, cancel, window, cx| vec![cancel(window, cx)])
//                 .overlay_closable(false)
//                 .show_close(false)
//             }
//         );
//         return;
//     };

//     let name = name.to_string();
//     window.open_modal(cx, move |modal, window, cx| {
        

//         with_launcher_context_mut(cx, |context, cx| {
//             println!("running modal {}", rand::rng().next_u32());
//             let version_manifest = context.meta.minecraft_version_manifest();

//             let message = match version_manifest {
//                 crate::meta::MetaResult::Pending => {
//                     "Manifest Pending".to_string()
//                 },
//                 crate::meta::MetaResult::Loaded(manifest) => {
//                     let version = manifest.versions.iter().find(|v| v.id == instance.version);

//                     if let Some(version) = version {
//                         let version_info = context.meta.minecraft_version_info(&version);

//                         match version_info {
//                             crate::meta::MetaResult::Pending => {
//                                 "Version Info Pending".to_string()
//                             },
//                             crate::meta::MetaResult::Loaded(version_info) => {
//                                 format!("{version_info:?}")
//                             },
//                             crate::meta::MetaResult::Error(meta_load_error) => {
//                                 "Version Info Error".to_string()
//                             },
//                         }

//                     } else {
//                         "Can't find version".to_string()
//                     }
//                 },
//                 crate::meta::MetaResult::Error(meta_load_error) => {
//                     "Manifest Error".to_string()
//                 },
//             };

//             modal.title(format!("Launching {}", name))
//                 .footer(|_, cancel, window, cx| vec![cancel(window, cx)])
//                 .child(message)
//                 .overlay_closable(false)
//                 .show_close(false)
//         })
//     });
// }