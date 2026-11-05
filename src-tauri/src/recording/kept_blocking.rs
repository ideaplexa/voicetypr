//! Explicit deletion and exit I/O never run on the UI thread or under STORE.
use super::*;

// Below the recorder's eight-second stop/join budget. An individual filesystem
// call cannot be cancelled, so exit stops waiting rather than joining that call.
pub(super) const EXIT_BUDGET: Duration = Duration::from_secs(2);
const DELETE_ERROR: &str = "Could not delete failed recording";

pub(super) struct PendingDelete {
    pub(super) path: PathBuf,
    pub(super) durable: bool,
    completion: tokio::sync::watch::Sender<Option<bool>>,
}

enum DeleteStart {
    Missing,
    Waiting(tokio::sync::watch::Receiver<Option<bool>>),
    Ready(Box<Deletion>),
}

struct Deletion {
    clip: Option<Clip>,
    position: usize,
    store: &'static Mutex<Store>,
    completion: tokio::sync::watch::Sender<Option<bool>>,
}

impl Deletion {
    fn begin(store: &'static Mutex<Store>, id: &str) -> Result<DeleteStart, String> {
        let mut owner = store.lock().unwrap_or_else(|e| e.into_inner());
        if owner.closed {
            return Err(DELETE_ERROR.into());
        }
        if let Some(pending) = owner.deleting.get(id) {
            return Ok(DeleteStart::Waiting(pending.completion.subscribe()));
        }
        let Some(position) = owner.clips.iter().position(|c| c.recovery.id == id) else {
            return Ok(DeleteStart::Missing);
        };
        if owner
            .active_retry
            .as_ref()
            .is_some_and(|(active, _, _)| active == id)
        {
            owner.cancel_retry();
        }
        let clip = owner.clips.remove(position).unwrap();
        let (completion, _) = tokio::sync::watch::channel(None);
        owner.deleting.insert(
            id.into(),
            PendingDelete {
                path: clip.path.clone(),
                durable: clip.disk.is_some(),
                completion: completion.clone(),
            },
        );
        drop(owner);
        Ok(DeleteStart::Ready(Box::new(Self {
            clip: Some(clip),
            position,
            store,
            completion,
        })))
    }

    fn finish(mut self, committed: bool) -> bool {
        if committed {
            let clip = self.clip.take().unwrap();
            let mut owner = self.store.lock().unwrap_or_else(|e| e.into_inner());
            owner.deleting.remove(&clip.recovery.id);
            if !owner.closed {
                resolve_clip(&clip, "discarded");
                owner.retire_ui_id(&clip.recovery.id);
            }
            self.completion.send_replace(Some(true));
        }
        committed // Failure, panic and cancelled awaiting tasks restore via Drop.
    }
}

impl Drop for Deletion {
    fn drop(&mut self) {
        let Some(mut clip) = self.clip.take() else {
            return;
        };
        let mut owner = self.store.lock().unwrap_or_else(|e| e.into_inner());
        owner.deleting.remove(&clip.recovery.id);
        if owner.closed {
            if clip.disk.is_some() {
                if let Some(files) = &clip._capture_files {
                    files.preserve_on_exit();
                }
            }
            self.completion.send_replace(Some(false));
            return;
        }
        clip.retire_after_retry = true;
        clip.busy = false;
        clip.retry_generation = None;
        clip.terminal = Default::default();
        let position = self.position.min(owner.clips.len());
        owner.clips.insert(position, clip);
        self.completion.send_replace(Some(false));
    }
}

async fn wait_for_deletion(
    mut result: tokio::sync::watch::Receiver<Option<bool>>,
) -> Result<(), String> {
    match result.wait_for(|value| value.is_some()).await {
        Ok(value) if *value == Some(true) => Ok(()),
        _ => Err(DELETE_ERROR.into()),
    }
}

async fn run_deletion(
    mut deletion: Deletion,
    commit: impl FnOnce(&mut Clip) -> bool + Send + 'static,
) -> Result<(), String> {
    let committed = tauri::async_runtime::spawn_blocking(move || {
        let committed = commit(deletion.clip.as_mut().unwrap());
        deletion.finish(committed)
    })
    .await
    .map_err(|_| DELETE_ERROR.to_owned())?;
    if committed {
        Ok(())
    } else {
        Err(DELETE_ERROR.into())
    }
}

pub(super) async fn discard<R: crate::commands::recording_runtime::RecordingRuntime>(
    app: AppHandle<R>,
    id: String,
) -> Result<(), String> {
    let generation = {
        let owner = STORE.lock().unwrap_or_else(|e| e.into_inner());
        owner
            .active_retry
            .as_ref()
            .filter(|(active, _, _)| active == &id)
            .map(|(_, g, _)| *g)
    };
    // Retire the retry phase before disk I/O, allowing a fresh take while Delete
    // is blocked. Its eventual completion still owns only this old generation.
    completion::cancelled(&app, generation);
    #[cfg(test)]
    let pause = app
        .try_state::<super::lifecycle_tests::DeletePause>()
        .map(|p| p.0.clone());
    let result = match Deletion::begin(&STORE, &id) {
        Ok(DeleteStart::Ready(deletion)) => {
            run_deletion(*deletion, move |clip| {
                #[cfg(test)]
                if let Some(pause) = pause {
                    pause.wait();
                }
                if !clip.commit_retirement() {
                    return false;
                }
                clip.remove_audio();
                true
            })
            .await
        }
        Ok(DeleteStart::Missing) => Ok(()),
        Ok(DeleteStart::Waiting(result)) => wait_for_deletion(result).await,
        Err(error) => Err(error),
    };
    completion::cancelled(&app, generation);
    if let Some(native) = R::native(&app) {
        crate::menu::runtime::refresh(native);
    }
    result
}

fn run_exit_work(budget: Duration, work: impl FnOnce(Instant) + Send + 'static) -> bool {
    let deadline = Instant::now() + budget;
    let (done, completed) = std::sync::mpsc::channel();
    if std::thread::Builder::new()
        .name("recovery-exit".into())
        .spawn(move || {
            work(deadline);
            let _ = done.send(());
        })
        .is_err()
    {
        return false;
    }
    completed
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .is_ok()
}

fn take_for_exit(store: &Mutex<Store>) -> Store {
    let mut owner = store.lock().unwrap_or_else(|e| e.into_inner());
    // Detached file cleanup must not emit after analytics shutdown. Durable
    // clips remain unresolved on disk and receive fresh terminals on restart.
    for clip in &owner.clips {
        clip.terminal
            .store(true, std::sync::atomic::Ordering::Release);
    }
    owner.cancel_retry();
    // Detach before any file access. Slow commits cannot retain STORE.
    let mut owned = std::mem::replace(
        &mut *owner,
        Store {
            closed: true,
            ..Default::default()
        },
    );
    owned.app = None; // No late UI publication after the exit wait expires.
    owned
}

pub(super) fn cleanup() {
    if !run_exit_work(EXIT_BUDGET, move |deadline| {
        let mut owned = take_for_exit(&STORE);
        owned.preserve_on_exit_until(deadline);
        owned.clear();
    }) {
        log::warn!("Recovery exit wait ended; uncommitted originals remain recoverable");
    }
}

#[cfg(test)]
#[path = "kept_blocking_tests.rs"]
mod tests;
