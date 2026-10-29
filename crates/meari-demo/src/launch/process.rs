//! A child process that is always stopped when dropped (SIGTERM first, then kill).
use std::process::Child;
use std::time::{Duration, Instant};

pub struct Owned {
    pub label: &'static str,
    child: Child,
}

impl Owned {
    pub fn new(label: &'static str, child: Child) -> Self {
        Self { label, child }
    }

    pub fn exited(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(Some(_)))
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        if self.exited() {
            return;
        }
        terminate(self.child.id());
        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            if self.exited() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn terminate(pid: u32) {
    use meari_platform::OsPolicy;
    meari_platform::current().request_terminate(pid);
}
