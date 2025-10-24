use std::{fmt::Debug, ops::Deref, sync::{atomic::{AtomicBool, AtomicPtr, AtomicU32, AtomicU64, Ordering}, Arc, RwLock}, time::Instant};

use atomic_time::AtomicOptionInstant;
use gpui::SharedString;
use rust_i18n::AtomicStr;

use crate::{backend::metadata::{manager::MetaLoadError, schemas::version_manifest::MinecraftVersionManifest}, frontend::instance::instance_page::Loader};

#[derive(Debug)]
pub enum MessageToBackend {
    LoadVersionManifest {
        reload: bool
    },
    CreateInstance {
        name: SharedString,
        version: SharedString,
        loader: Loader,
    },
    StartInstance {
        name: SharedString,
        progress_trackers: ProgressTrackers
    },
    DownloadAllMetadata
}

#[derive(Debug)]
pub enum MessageToFrontend {
    VersionManifestUpdated(Result<Arc<MinecraftVersionManifest>, MetaLoadError>),
    InstanceAdded {
        name: SharedString,
        version: SharedString,
        loader: Loader
    },
    Refresh
}

#[derive(Default, Clone, Debug)]
pub struct ProgressTrackers {
    pub trackers: Arc<RwLock<Vec<ProgressTracker>>>,
}

impl ProgressTrackers {
    pub fn push(&self, tracker: ProgressTracker) {
        self.trackers.write().unwrap().push(tracker);
    }
}

#[derive(Clone, Debug)]
pub struct ProgressTracker {
    inner: Arc<ProgressTrackerInner>,
    sender: tokio::sync::mpsc::Sender<MessageToFrontend>
}

struct ProgressTrackerInner {
    count: AtomicU32,
    total: AtomicU32,
    finished_at: AtomicOptionInstant,
    finished_with_error: AtomicBool,
    title: RwLock<SharedString>,
}

// #[derive(Debug, Clone)]
// pub struct ArcStrWrapper(Arc<str>);

// unsafe impl RefCnt for ArcStrWrapper {
//     type Base = str;

//     fn into_ptr(me: Self) -> *mut Self::Base {
//         todo!()
//     }

//     fn as_ptr(me: &Self) -> *mut Self::Base {
//         todo!()
//     }

//     unsafe fn from_ptr(ptr: *const Self::Base) -> Self {
//         todo!()
//     }
// }

impl Debug for ProgressTrackerInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProgressTrackerInner")
            .field("count", &self.count)
            .field("total", &self.total)
            .field("finished_at", &self.finished_at.load(Ordering::Relaxed))
            .finish()
    }
}

impl ProgressTracker {
    pub fn new(title: SharedString, sender: tokio::sync::mpsc::Sender<MessageToFrontend>) -> Self {
        Self {
            inner: Arc::new(ProgressTrackerInner {
                count: AtomicU32::new(0),
                total: AtomicU32::new(0),
                finished_at: AtomicOptionInstant::none(),
                finished_with_error: AtomicBool::new(false),
                title: RwLock::new(title),
            }),
            sender
        }
    }

    pub fn get_title(&self) -> SharedString {
        self.inner.title.read().unwrap().clone()
    }

    pub fn get_shared_title(&self) -> SharedString {
        let arc: Arc<str> = self.get_title().into();
        SharedString::new(arc)
    }

    pub fn set_title(&self, title: SharedString) {
        *self.inner.title.write().unwrap() = title;
    }

    pub fn get_float(&self) -> Option<f32> {
        let (count, total) = self.get();
        if total == 0 {
            None
        } else {
            Some((count as f32 / total as f32).clamp(0.0, 1.0))
        }
    }

    pub fn get(&self) -> (u32, u32) {
        (
            self.inner.count.load(Ordering::SeqCst),
            self.inner.total.load(Ordering::SeqCst)
        )
    }

    pub fn set_finished(&self, error: bool) {
        if error {
            self.inner.finished_with_error.store(true, Ordering::SeqCst);
        }
        self.inner.finished_at.compare_exchange(None, Some(Instant::now()), Ordering::SeqCst, Ordering::Relaxed);
    }

    pub fn get_finished_at(&self) -> Option<Instant> {
        self.inner.finished_at.load(Ordering::SeqCst)
    }

    pub fn is_error(&self) -> bool {
        self.inner.finished_with_error.load(Ordering::SeqCst)
    }

    pub fn add_count(&self, count: u32) {
        self.inner.count.fetch_add(count, Ordering::SeqCst);
    }

    pub fn set_count(&self, count: u32) {
        self.inner.count.store(count, Ordering::SeqCst);
    }

    pub fn add_total(&self, total: u32) {
        self.inner.total.fetch_add(total, Ordering::SeqCst);
    }

    pub fn set_total(&self, total: u32) {
        self.inner.total.store(total, Ordering::SeqCst);
    }

    pub async fn notify(&self) {
        let _ = self.sender.send(MessageToFrontend::Refresh).await;
    }
}