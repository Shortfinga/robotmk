use super::termination::TerminationFlag;

use anyhow::{bail, Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use fs4::{lock_contended_error, FileExt};
use log::debug;
use std::fs::File;
use std::io::Result as IOResult;
use std::time::Duration;

#[derive(Clone)]
pub struct Locker {
    lock_path: Utf8PathBuf,
    termination_flag: Option<TerminationFlag>,
}

pub struct Lock(File);

impl Locker {
    pub fn new(
        lock_path: impl AsRef<Utf8Path>,
        termination_flag: Option<&TerminationFlag>,
    ) -> Self {
        Self {
            lock_path: lock_path.as_ref().to_owned(),
            termination_flag: termination_flag.cloned(),
        }
    }

    pub fn wait_for_read_lock(&self) -> Result<Lock> {
        debug!("Waiting for read lock");
        let file = self.file()?;
        if let Some(termination_flag) = &self.termination_flag {
            Self::lock_manual_loop(&(|| file.try_lock_shared()), termination_flag)
        } else {
            file.lock_shared()
                .context("Unexpected error while attempting to acquire read lock")
        }
        .context("Failed to acquire read lock")?;
        debug!("Got read lock");
        Ok(Lock(file))
    }

    pub fn wait_for_write_lock(&self) -> Result<Lock> {
        debug!("Waiting for write lock");
        let file = self.file()?;
        if let Some(termination_flag) = &self.termination_flag {
            Self::lock_manual_loop(&(|| file.try_lock_exclusive()), termination_flag)
        } else {
            file.lock_exclusive()
                .context("Unexpected error while attempting to acquire write lock")
        }
        .context("Failed to acquire write lock")?;
        debug!("Got write lock");
        Ok(Lock(file))
    }

    fn file(&self) -> Result<File> {
        File::open(&self.lock_path).context(format!(
            "Failed to open {} for creating lock",
            self.lock_path,
        ))
    }

    fn lock_manual_loop(
        lock_tryer: &dyn Fn() -> IOResult<()>,
        termination_flag: &TerminationFlag,
    ) -> Result<()> {
        loop {
            if termination_flag.should_terminate() {
                bail!("Terminated")
            }
            match lock_tryer() {
                Ok(lock) => return Ok(lock),
                Err(error) => {
                    if error.kind() == lock_contended_error().kind() {
                        std::thread::sleep(Duration::from_millis(250))
                    } else {
                        return Err(error)
                            .context("Unexpected error while attempting to acquire lock");
                    }
                }
            }
        }
    }
}

impl Lock {
    pub fn release(self) -> Result<()> {
        self.0.unlock().context("Failed to release lock")
    }
}