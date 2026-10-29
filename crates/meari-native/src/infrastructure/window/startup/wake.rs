use super::{NativeStartupError, NativeWindowWake};
use blitz_shell::BlitzShellProxy;
use std::sync::{Arc, Mutex};
enum Route {
    Startup,
    Preparing,
    Attached(NativeWindowWake),
    Closed,
}
struct State {
    route: Route,
    dirty: bool,
}
pub(super) struct Shared {
    proxy: BlitzShellProxy,
    state: Mutex<State>,
}
impl Shared {
    pub fn new(proxy: BlitzShellProxy) -> Arc<Self> {
        Arc::new(Self {
            proxy,
            state: Mutex::new(State {
                route: Route::Startup,
                dirty: false,
            }),
        })
    }
    pub fn notify(&self) -> Result<bool, NativeStartupError> {
        let target = {
            let mut state = self.state.lock().map_err(|_| NativeStartupError::Busy)?;
            match &state.route {
                Route::Closed => return Err(NativeStartupError::Closed),
                Route::Attached(wake) => Some(wake.clone()),
                Route::Startup | Route::Preparing => {
                    if state.dirty {
                        return Ok(false);
                    }
                    state.dirty = true;
                    None
                }
            }
        };
        if let Some(target) = target {
            target.wake().map_err(|_| NativeStartupError::Closed)
        } else {
            self.proxy.wake_up();
            Ok(true)
        }
    }
    pub fn take(&self) -> Result<bool, NativeStartupError> {
        let mut state = self.state.lock().map_err(|_| NativeStartupError::Busy)?;
        match state.route {
            Route::Startup | Route::Preparing => Ok(std::mem::take(&mut state.dirty)),
            Route::Attached(_) => Err(NativeStartupError::Stale),
            Route::Closed => Err(NativeStartupError::Closed),
        }
    }
    pub fn preparing(&self) -> Result<(), NativeStartupError> {
        let mut state = self.state.lock().map_err(|_| NativeStartupError::Busy)?;
        if !matches!(state.route, Route::Startup) {
            return Err(NativeStartupError::AlreadyIssued);
        }
        state.route = Route::Preparing;
        Ok(())
    }
    pub fn bind(&self, wake: NativeWindowWake) -> Result<(), NativeStartupError> {
        if wake.closed.load(std::sync::atomic::Ordering::Acquire) {
            let mut state = self.state.lock().map_err(|_| NativeStartupError::Busy)?;
            state.route = Route::Closed;
            return Err(NativeStartupError::Closed);
        }
        let dirty = {
            let mut state = self.state.lock().map_err(|_| NativeStartupError::Busy)?;
            if !matches!(state.route, Route::Preparing) {
                return Err(NativeStartupError::Stale);
            }
            state.route = Route::Attached(wake.clone());
            std::mem::take(&mut state.dirty)
        };
        if dirty && wake.wake().is_err() {
            let mut state = self.state.lock().map_err(|_| NativeStartupError::Busy)?;
            state.route = Route::Closed;
            state.dirty = true;
            return Err(NativeStartupError::Closed);
        }
        Ok(())
    }
    pub fn close(&self) {
        let target = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            state.dirty = false;
            std::mem::replace(&mut state.route, Route::Closed)
        };
        drop(target);
    }
}
#[derive(Clone)]
pub struct NativeStartupWake {
    pub(super) shared: Arc<Shared>,
}
impl NativeStartupWake {
    pub fn notify(&self) -> Result<bool, NativeStartupError> {
        self.shared.notify()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binding_failure_keeps_dirty_and_revokes_the_failed_generation() {
        let startup = super::super::tests::startup();
        let (target, _) = super::super::tests::target();
        startup.signal.notify().unwrap();
        startup.signal.preparing().unwrap();
        target.close();
        assert!(matches!(
            startup.signal.bind(target),
            Err(NativeStartupError::Closed)
        ));
        assert!(startup.signal.state.lock().unwrap().dirty);
        assert!(matches!(
            startup.signal.notify(),
            Err(NativeStartupError::Closed)
        ));
    }
}
