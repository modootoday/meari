use super::frame::error;
use wasm_bindgen::{JsCast, closure::Closure};
use web_sys::{Event, EventTarget};

pub(super) struct Listener {
    target: EventTarget,
    callback: Closure<dyn FnMut(Event)>,
    kind: &'static str,
}

impl Listener {
    pub fn capture(
        target: EventTarget,
        kind: &'static str,
        callback: impl FnMut(Event) + 'static,
    ) -> Result<Self, String> {
        let callback = Closure::wrap(Box::new(callback) as Box<dyn FnMut(Event)>);
        target
            .add_event_listener_with_callback_and_bool(
                kind,
                callback.as_ref().unchecked_ref(),
                true,
            )
            .map_err(error)?;
        Ok(Self {
            target,
            callback,
            kind,
        })
    }
}
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.target.remove_event_listener_with_callback_and_bool(
            self.kind,
            self.callback.as_ref().unchecked_ref(),
            true,
        );
    }
}
