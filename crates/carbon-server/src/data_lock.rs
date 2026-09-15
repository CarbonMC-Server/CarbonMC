//! Process-lifetime ownership of Carbon's working data directory.
use anyhow::Context;
use std::{
    fs::{File, OpenOptions},
    path::Path,
};

pub(crate) struct DataLock {
    _file: File,
}
impl DataLock {
    pub(crate) fn acquire(directory: &Path) -> anyhow::Result<Self> {
        let path = directory.join(".carbon-data.lock");
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .with_context(|| format!("cannot open data lock {}", path.display()))?;
        fs2::FileExt::try_lock_exclusive(&file).with_context(|| {
            format!(
                "cannot exclusively lock {}; another Carbon process may own this data directory",
                directory.display()
            )
        })?;
        // Do not remove the lock file on drop: unlinking it allows two separate
        // lock-file identities and defeats exclusion. Closing releases the OS lock.
        Ok(Self { _file: file })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exclusive_ownership_is_released_on_drop() {
        let directory = std::env::temp_dir().join(format!("carbon-lock-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let first = DataLock::acquire(&directory).unwrap();
        assert!(DataLock::acquire(&directory).is_err());
        // Directory aliases resolve to the same lock-file identity.
        assert!(DataLock::acquire(&directory.join(".")).is_err());
        drop(first);
        let second = DataLock::acquire(&directory).unwrap();
        assert!(directory.join(".carbon-data.lock").exists());
        drop(second);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn separate_directories_can_be_owned_independently() {
        let directory = std::env::temp_dir().join(format!("carbon-lock-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let child = directory.join("other");
        std::fs::create_dir(&child).unwrap();
        let first = DataLock::acquire(&directory).unwrap();
        let second = DataLock::acquire(&child).unwrap();
        drop((first, second));
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[cfg(test)]
mod process_tests {
    use super::*;
    use std::{
        process::{Child, Command},
        time::{Duration, Instant},
    };
    struct ChildGuard(Child);
    impl Drop for ChildGuard {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    #[ignore = "subprocess entry point invoked by crash_releases_process_lock"]
    fn lock_child() {
        let directory = std::path::PathBuf::from(
            std::env::var_os("CARBON_LOCK_TEST_DIRECTORY").expect("test directory"),
        );
        let _lock = DataLock::acquire(&directory).unwrap();
        std::fs::write(directory.join("ready"), b"locked").unwrap();
        loop {
            std::thread::park();
        }
    }
    #[test]
    fn crash_releases_process_lock() {
        let directory =
            std::env::temp_dir().join(format!("carbon-lock-process-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let mut child = ChildGuard(
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "data_lock::process_tests::lock_child",
                    "--ignored",
                ])
                .env("CARBON_LOCK_TEST_DIRECTORY", &directory)
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(15);
        while !directory.join("ready").exists() {
            assert!(
                child.0.try_wait().unwrap().is_none(),
                "child exited before acquiring lock"
            );
            assert!(Instant::now() < deadline, "child lock deadline exceeded");
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(DataLock::acquire(&directory).is_err());
        child.0.kill().unwrap();
        child.0.wait().unwrap();
        drop(child);
        let recovered = DataLock::acquire(&directory).unwrap();
        drop(recovered);
        std::fs::remove_dir_all(directory).unwrap();
    }
}

#[cfg(test)]
mod ownership_tests {
    use super::*;
    #[test]
    fn shared_state_retains_lock_until_last_writer_is_dropped() {
        let directory =
            std::env::temp_dir().join(format!("carbon-lock-state-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&directory).unwrap();
        let lock = DataLock::acquire(&directory).unwrap();
        let mut state =
            crate::state::ServerState::new("test".into(), 0, tokio::sync::watch::channel(false).0);
        state.retain_data_lock(lock);
        let owner = std::sync::Arc::new(state);
        let background_writer = owner.clone();
        drop(owner);
        assert!(DataLock::acquire(&directory).is_err());
        drop(background_writer);
        drop(DataLock::acquire(&directory).unwrap());
        std::fs::remove_dir_all(directory).unwrap();
    }
}
