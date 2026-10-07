use crate::{Error, Result, Store, backup::RestoreFault};
use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

type Job = Box<dyn FnOnce(&mut Store, &mut bool) + Send>;
pub struct Actor {
    sender: Option<mpsc::Sender<Job>>,
    frozen: Arc<Mutex<bool>>,
    worker: Option<JoinHandle<()>>,
}
impl Actor {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_interval(root, Duration::from_secs(60))
    }
    fn open_with_interval(root: impl AsRef<Path>, interval: Duration) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let (sender, receiver) = mpsc::channel::<Job>();
        let (ready_tx, ready_rx) = mpsc::channel();
        let worker = thread::spawn(move || match Store::open(root) {
            Ok(mut store) => {
                let _ = ready_tx.send(Ok(()));
                let mut failed_write = false;
                let mut next = Instant::now() + interval;
                loop {
                    match receiver.recv_timeout(next.saturating_duration_since(Instant::now())) {
                        Ok(job) => job(&mut store, &mut failed_write),
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    if Instant::now() >= next {
                        let _ = store.run_automatic_backup(false);
                        next = Instant::now() + interval;
                    }
                }
            }
            Err(error) => {
                let _ = ready_tx.send(Err(error));
            }
        });
        ready_rx.recv().map_err(|_| Error::Unavailable)??;
        Ok(Self {
            sender: Some(sender),
            frozen: Arc::new(Mutex::new(false)),
            worker: Some(worker),
        })
    }
    pub fn submit<T: Send + 'static>(
        &self,
        write: bool,
        action: impl FnOnce(&mut Store) -> Result<T> + Send + 'static,
    ) -> Result<mpsc::Receiver<Result<T>>> {
        let gate = self.frozen.lock().map_err(|_| Error::Unavailable)?;
        if *gate {
            return Err(Error::Restoring);
        }
        let (tx, rx) = mpsc::channel();
        self.sender
            .as_ref()
            .ok_or(Error::Unavailable)?
            .send(Box::new(move |store, failed| {
                let result = action(store);
                if write && result.is_err() {
                    *failed = true;
                }
                let _ = tx.send(result);
            }))
            .map_err(|_| Error::Unavailable)?;
        Ok(rx)
    }
    pub fn call<T: Send + 'static>(
        &self,
        write: bool,
        action: impl FnOnce(&mut Store) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.submit(write, action)?
            .recv()
            .map_err(|_| Error::Unavailable)?
    }
    pub fn acknowledge_saved(&self) -> Result<()> {
        // Frontend calls this only after retry/flush succeeds for every dirty editor.
        let gate = self.frozen.lock().map_err(|_| Error::Unavailable)?;
        if *gate {
            return Err(Error::Restoring);
        }
        let (tx, rx) = mpsc::channel();
        self.sender
            .as_ref()
            .ok_or(Error::Unavailable)?
            .send(Box::new(move |store, failed| {
                let result = store.flush();
                if result.is_ok() {
                    *failed = false;
                }
                let _ = tx.send(result);
            }))
            .map_err(|_| Error::Unavailable)?;
        drop(gate);
        rx.recv().map_err(|_| Error::Unavailable)?
    }
    pub fn restore(
        &self,
        package: PathBuf,
        sha256: String,
        fault: RestoreFault,
    ) -> Result<mpsc::Receiver<Result<()>>> {
        let mut gate = self.frozen.lock().map_err(|_| Error::Unavailable)?;
        if *gate {
            return Err(Error::Restoring);
        }
        *gate = true;
        let frozen = self.frozen.clone();
        let (tx, rx) = mpsc::channel();
        if self
            .sender
            .as_ref()
            .ok_or(Error::Unavailable)?
            .send(Box::new(move |store, failed| {
                let result = if *failed {
                    Err(Error::Unsaved)
                } else {
                    store.restore(&package, &sha256, fault)
                };
                if store.conn().is_ok()
                    && let Ok(mut gate) = frozen.lock()
                {
                    *gate = false;
                }
                let _ = tx.send(result);
            }))
            .is_err()
        {
            *gate = false;
            return Err(Error::Unavailable);
        }
        Ok(rx)
    }
}
impl Drop for Actor {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn automatic_backup_runs_even_when_reads_keep_arriving() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.test-output");
        std::fs::create_dir_all(&root).unwrap();
        let temp = tempfile::TempDir::new_in(root).unwrap();
        let actor = Actor::open_with_interval(temp.path(), Duration::from_millis(20)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let status = actor.call(false, |s| s.automatic_backup_status()).unwrap();
            assert!(status.last_error.is_none(), "{:?}", status.last_error);
            if status.managed_count == 2 {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Scheduler was starved by queued jobs"
            );
            actor.call(false, |s| s.profile()).unwrap();
        }
    }
}
