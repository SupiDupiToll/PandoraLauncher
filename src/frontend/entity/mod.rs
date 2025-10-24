use gpui::Entity;

use crate::{backend::BackendHandle, frontend::entity::{instance::InstanceEntries, version::VersionEntries}};

pub mod instance;
pub mod version;

#[derive(Clone)]
pub struct DataEntities {
    pub instances: Entity<InstanceEntries>,
    pub versions: Entity<VersionEntries>,
    pub backend_handle: BackendHandle
}