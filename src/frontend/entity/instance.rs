use gpui::{AppContext, Context, Entity, EventEmitter, SharedString};

pub struct InstanceEntries {
    pub entries: Vec<InstanceEntry>,
}

impl InstanceEntries {
    pub fn add<C: AppContext>(entity: &Entity<Self>, entry: InstanceEntry, cx: &mut C) {
        entity.update(cx, |entries, cx| {
            entries.entries.push(entry.clone());
            cx.emit(InstanceAddedEvent { 
                instance: entry
            });
        });
    }
}

#[derive(Clone)]
pub struct InstanceEntry {
    pub name: SharedString,
    pub version: SharedString
}

impl EventEmitter<InstanceAddedEvent> for InstanceEntries {}

pub struct InstanceAddedEvent {
    pub instance: InstanceEntry
}