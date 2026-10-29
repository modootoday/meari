use crate::domain::host::HostEnvironment;
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, closure::Closure};

type Pending = Rc<RefCell<Option<Result<HostEnvironment, String>>>>;

struct Listener {
    target: web_sys::EventTarget,
    name: &'static str,
    callback: Closure<dyn FnMut(web_sys::Event)>,
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self
            .target
            .remove_event_listener_with_callback(self.name, self.callback.as_ref().unchecked_ref());
    }
}

/// Coalesces host events into the latest observation for the application scheduler.
/// No component callback runs inside the browser's viewport measurement event.
pub struct BrowserHostObserver {
    pending: Pending,
    listeners: Vec<Listener>,
}

impl BrowserHostObserver {
    pub fn new() -> Result<Self, String> {
        let window = web_sys::window().ok_or("browser Window unavailable")?;
        let visual = window
            .visual_viewport()
            .ok_or("VisualViewport unavailable")?;
        let color = window
            .match_media("(prefers-color-scheme: dark)")
            .map_err(|error| format!("color query failed: {error:?}"))?
            .ok_or("color query unavailable")?;
        let pending = Rc::new(RefCell::new(Some(super::snapshot())));
        let mut observer = Self {
            pending,
            listeners: Vec::new(),
        };
        for (target, name) in [
            (web_sys::EventTarget::from(window), "resize"),
            (web_sys::EventTarget::from(visual.clone()), "resize"),
            (web_sys::EventTarget::from(visual), "scroll"),
            (web_sys::EventTarget::from(color), "change"),
        ] {
            let pending = observer.pending.clone();
            let callback = Closure::new(move |_: web_sys::Event| {
                *pending.borrow_mut() = Some(super::snapshot());
            });
            target
                .add_event_listener_with_callback(name, callback.as_ref().unchecked_ref())
                .map_err(|error| format!("host listener registration failed: {error:?}"))?;
            observer.listeners.push(Listener {
                target,
                name,
                callback,
            });
        }
        Ok(observer)
    }

    pub fn take_pending(&mut self) -> Option<Result<HostEnvironment, String>> {
        self.pending.borrow_mut().take()
    }

    pub fn close(&mut self) {
        self.listeners.clear();
        self.pending.borrow_mut().take();
    }
}
