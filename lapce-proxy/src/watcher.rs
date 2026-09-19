use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crossbeam_channel::{Receiver, unbounded};
use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher,
    event::{ModifyKind, RenameMode},
    recommended_watcher,
};
use parking_lot::Mutex;

/// Wrapper around a `notify::Watcher`. It runs the inner watcher
/// in a separate thread, and communicates with it via a [crossbeam channel].
/// [crossbeam channel]: https://docs.rs/crossbeam-channel
pub struct FileWatcher {
    rx_event: Option<Receiver<Result<Event, notify::Error>>>,
    inner: RecommendedWatcher,
    state: Arc<Mutex<WatcherState>>,
}

#[derive(Debug, Default)]
struct WatcherState {
    watchees: Vec<Watchee>,
}

/// Tracks a registered 'that-which-is-watched'.
#[doc(hidden)]
struct Watchee {
    path: PathBuf,
    recursive: bool,
    token: WatchToken,
}

/// Token provided to `FileWatcher`, to associate events with
/// interested parties.
///
/// Note: `WatchToken`s are assumed to correspond with an
/// 'area of interest'; that is, they are used to route delivery
/// of events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchToken(pub usize);

/// A trait for types which can be notified of new events.
/// New events are accessible through the `FileWatcher` instance.
pub trait Notify: Send {
    fn notify(&self, events: Vec<(WatchToken, Event)>);
}

impl FileWatcher {
    pub fn new() -> Self {
        let (tx_event, rx_event) = unbounded();

        let state = Arc::new(Mutex::new(WatcherState::default()));

        let inner = recommended_watcher(tx_event).expect("watcher should spawn");

        FileWatcher {
            rx_event: Some(rx_event),
            inner,
            state,
        }
    }

    pub fn notify<T: Notify + 'static>(&mut self, peer: T) {
        let rx_event = self.rx_event.take().unwrap();
        let state = self.state.clone();
        std::thread::spawn(move || {
            while let Ok(Ok(event)) = rx_event.recv() {
                let mut events = Vec::new();
                {
                    let mut state = state.lock();
                    let WatcherState {
                        ref mut watchees, ..
                    } = *state;

                    watchees
                        .iter()
                        .filter(|w| w.wants_event(&event))
                        .map(|w| w.token)
                        .for_each(|t| events.push((t, event.clone())));
                }

                peer.notify(events);
            }
        });
    }

    /// Begin watching `path`. As `Event`s (documented in the
    /// [notify](https://docs.rs/notify) crate) arrive, they are stored
    /// with the associated `token` and a task is added to the runloop's
    /// idle queue.
    ///
    /// Delivery of events then requires that the runloop's handler
    /// correctly forward the `handle_idle` call to the interested party.
    pub fn watch(&mut self, path: &Path, recursive: bool, token: WatchToken) {
        self.watch_impl(path, recursive, token);
    }

    fn watch_impl(&mut self, path: &Path, recursive: bool, token: WatchToken) {
        let path = match path.canonicalize() {
            Ok(ref p) => p.to_owned(),
            Err(_) => {
                return;
            }
        };

        let mut state = self.state.lock();

        let w = Watchee {
            path,
            recursive,
            token,
        };
        let mode = mode_from_bool(w.recursive);

        if !state.watchees.iter().any(|w2| w.path == w2.path) {
            if let Err(err) = self.inner.watch(&w.path, mode) {
                tracing::error!("{:?}", err);
            }
        }

        state.watchees.push(w);
    }
}

impl Default for FileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Watchee {
    fn wants_event(&self, event: &Event) -> bool {
        match &event.kind {
            EventKind::Modify(ModifyKind::Name(RenameMode::Both)) => {
                if event.paths.len() == 2 {
                    //There will be two paths. First is "from" and other is "to".
                    self.applies_to_path(&event.paths[0])
                        || self.applies_to_path(&event.paths[1])
                } else {
                    false
                }
            }
            EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_) => {
                event.paths.first().map(|p| self.applies_to_path(p)) == Some(true)
            }
            _ => false,
        }
    }

    fn applies_to_path(&self, path: &Path) -> bool {
        if !path.starts_with(&self.path) {
            return false;
        }
        (self.recursive || self.path == path)
            || path.parent() == Some(self.path.as_path())
    }
}
impl std::fmt::Debug for Watchee {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "Watchee path: {:?}, r {}, t {}",
            self.path, self.recursive, self.token.0
        )
    }
}

fn mode_from_bool(is_recursive: bool) -> RecursiveMode {
    if is_recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    }
}
