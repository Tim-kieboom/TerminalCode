use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use crate::pty::{Session, Shell, SpawnConfig};

/// A session running `script`, and how often the app was woken.
pub(super) struct Rig {
    pub(super) session: Session,
    pub(super) wakes: Arc<AtomicUsize>,
    pub(super) directory: std::path::PathBuf,
    _dir: tempfile::TempDir,
}

pub(super) fn start(script: &str) -> Rig {
    start_shell(Shell::script(script))
}

pub(super) fn start_shell(shell: Shell) -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let directory = dir.path().canonicalize().unwrap();
    let wakes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&wakes);
    let session = Session::spawn(
        &SpawnConfig {
            shell,
            directory: directory.clone(),
            rows: 24,
            columns: 80,
        },
        move || {
            counter.fetch_add(1, Ordering::SeqCst);
        },
    )
    .unwrap();
    Rig {
        session,
        wakes,
        directory,
        _dir: dir,
    }
}

impl Rig {
    pub(super) fn text(&self) -> String {
        self.session.with_screen(|screen| screen.contents())
    }

    /// Waits until `condition` holds, for at most ten seconds.
    pub(super) fn wait_until(&self, what: &str, condition: impl Fn(&Rig) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !condition(self) {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for {what}; the screen shows {:?}",
                self.text()
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    pub(super) fn wait_for_text(&self, wanted: &str) {
        self.wait_until(wanted, |rig| rig.text().contains(wanted));
    }

    pub(super) fn wait_for_exit(&self) -> u32 {
        self.wait_until("the shell to exit", |rig| rig.session.exit_code().is_some());
        self.session.exit_code().unwrap()
    }
}
