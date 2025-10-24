use gpui::SharedString;

use crate::frontend::instance::instance_page::Loader;

pub struct Instance {
    pub name: SharedString,
    pub version: SharedString,
    pub loader: Loader
}