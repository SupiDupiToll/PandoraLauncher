
use gpui::{prelude::*, *};
use gpui_component::{
    button::Button, progress::Progress, table::{Column, ColumnSort, Table, TableDelegate}, v_flex, ContextModal, Sizable
};
use rust_i18n::t;

use crate::{backend::BackendHandle, bridge::{MessageToBackend, ProgressTrackers}, frontend::{component::progress_bar::{ProgressBar, ProgressBarColor}, entity::{instance::{InstanceAddedEvent, InstanceEntries, InstanceEntry}, DataEntities}, ui}, ts};

pub struct InstanceList {
    columns: Vec<Column>,
    items: Vec<InstanceEntry>,
    backend_handle: BackendHandle,
    _instance_added_subscription: Subscription,
}

impl InstanceList {
    pub fn create_table(data: &DataEntities, window: &mut Window, cx: &mut App) -> Entity<Table<Self>> {
        let instances = data.instances.clone();
        let items = instances.read(cx).entries.clone();
        cx.new(|cx| {
            let instance_added_subscription = cx.subscribe::<_, InstanceAddedEvent>(&instances, |table: &mut Table<InstanceList>, _, event, cx| {
                table.delegate_mut().items.insert(0, event.instance.clone());
                cx.notify();
            });
            let instance_list = Self {
                columns: vec![
                    Column::new("start", "")
                        .width(100.)
                        .fixed_left()
                        .movable(false)
                        .resizable(false),
                    Column::new("name", "Name")
                        .width(200.)
                        .fixed_left()
                        .sortable()
                        .resizable(true),
                    Column::new("version", "Version")
                        .width(200.)
                        .fixed_left()
                        .sortable()
                        .resizable(true),
                ],
                items,
                backend_handle: data.backend_handle.clone(),
                _instance_added_subscription: instance_added_subscription,
            };
            Table::new(instance_list, window, cx)
        })
    }
}

impl TableDelegate for InstanceList {
    fn columns_count(&self, cx: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, cx: &App) -> usize {
        self.items.len()
    }

    fn column(&self, col_ix: usize, cx: &App) -> &gpui_component::table::Column {
        &self.columns[col_ix]
    }

    fn perform_sort(
            &mut self,
            col_ix: usize,
            sort: gpui_component::table::ColumnSort,
            window: &mut Window,
            cx: &mut Context<Table<Self>>,
        ) {
        if let Some(col) = self.columns.get_mut(col_ix) {
            match col.key.as_ref() {
                "name" => self.items.sort_by(|a, b| match sort {
                    ColumnSort::Descending => lexical_sort::natural_lexical_cmp(&a.name, &b.name).reverse(),
                    _ => lexical_sort::natural_lexical_cmp(&a.name, &b.name),
                }),
                "version" => self.items.sort_by(|a, b| match sort {
                    ColumnSort::Descending => lexical_sort::natural_lexical_cmp(&a.version, &b.version).reverse(),
                    _ => lexical_sort::natural_lexical_cmp(&a.version, &b.version),
                }),
                _ => {}
            }
        }
    }

    fn render_td(
        &self,
        row_ix: usize,
        col_ix: usize,
        window: &mut Window,
        cx: &mut Context<gpui_component::table::Table<Self>>,
    ) -> impl IntoElement {
        let item = &self.items[row_ix];
        if let Some(col) = self.columns.get(col_ix) {
            match col.key.as_ref() {
                "name" => {
                    item.name.clone().into_any_element()
                },
                "version" => {
                    item.version.clone().into_any_element()
                },
                "start" => {
                    let backend_handle = self.backend_handle.clone();
                    Button::new("start")
                        .compact()
                        .small()
                        .label("Start")
                        .on_click({
                            let name = item.name.clone();
                            move |_, window, cx| {
                                let progress_trackers = ProgressTrackers::default();

                                backend_handle.send(MessageToBackend::StartInstance {
                                    name: name.clone(),
                                    progress_trackers: progress_trackers.clone(),
                                });

                                show_launching_modal(window, cx, name.clone(), progress_trackers);
                            }
                        })
                        .into_any_element()
                },
                _ => "Unknown".into_any_element()
            }
        } else {
            "Unknown".into_any_element()
        }

    }
}

pub fn show_launching_modal(window: &mut Window, cx: &mut App, name: SharedString, progress: ProgressTrackers) {
    let title = ts!("launch.launching", instance = name);

    window.open_modal(cx, move |modal, window, cx| {
        let trackers = progress.trackers.read().unwrap();
        let mut progress_entries = Vec::with_capacity(trackers.len());
        for tracker in &*trackers {
            let mut opacity = 1.0;

            let mut progress_bar = ProgressBar::new();
            if let Some(progress_amount) = tracker.get_float() {
                progress_bar.amount = progress_amount;
            }

            if let Some(finished_at) = tracker.get_finished_at() {
                let elapsed = finished_at.elapsed().as_secs_f32();
                if elapsed >= 2.0 {
                    continue;
                } else if elapsed >= 1.0 {
                    opacity = 2.0 - elapsed;
                }

                if tracker.is_error() {
                    progress_bar.color = ProgressBarColor::Error;
                } else {
                    progress_bar.color = ProgressBarColor::Success;
                }
                if elapsed <= 0.5 {
                    progress_bar.color_scale = elapsed * 2.0;
                }

                window.request_animation_frame();
            }

            let title = tracker.get_title();
            progress_entries.push(div().gap_3().child(title).child(progress_bar).opacity(opacity));
        }
        drop(trackers);

        let progress = v_flex().gap_2().children(progress_entries);

        modal.title(title.clone()).child(progress)
    });
}

