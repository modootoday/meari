#![cfg(all(windows, feature = "window"))]
//! Explicit OS gate. Run only in an allocated Windows native-window slot.
use meari_native::{
    application::{WindowOptions, window_lifecycle::*},
    domain::Viewport,
    infrastructure::{document::InteractiveDocument, native_host::*, window::*},
};
use std::{
    cell::{Cell, RefCell},
    io,
    rc::Rc,
    time::{Duration, Instant},
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    platform::windows::EventLoopBuilderExtWindows,
    window::WindowId,
};

#[path = "lifecycle.rs"]
mod lifecycle;
#[path = "parent.rs"]
mod parent;
#[path = "parent_client.rs"]
mod parent_client;
#[path = "preparation.rs"]
mod preparation;

fn document() -> NativeDocumentHandle {
    let source = meari::parse("type Props={};export function App(props:Props){return <main>Native child lifecycle</main>;}", meari::application::Language::Tsx).unwrap();
    NativeDocumentHandle::new(
        InteractiveDocument::new(
            source,
            "main{padding:12px}",
            Viewport {
                width: 360,
                height: 240,
                dark: false,
            },
        )
        .unwrap(),
        1,
    )
    .unwrap()
}
struct Host(Rc<Cell<usize>>, Rc<Cell<usize>>);
impl NativeWindowHost for Host {
    type Error = io::Error;
    type CloseCheckpoint = ();
    fn admit_callback(
        &mut self,
        _: &HostStamp,
        _: &meari::application::host_callbacks::PendingCallback,
    ) -> IntentAdmission<Self::Error> {
        self.1.set(self.1.get() + 1);
        IntentAdmission::Rejected(io::Error::other("technical native probe has no callbacks"))
    }
    fn after_dispatch(&mut self, _: HostTurn) -> Result<HostEffects, Self::Error> {
        self.1.set(self.1.get() + 1);
        Ok(HostEffects::default())
    }
    fn can_close(&mut self, _: &HostStamp, _: CloseCause) -> Result<bool, Self::Error> {
        self.1.set(self.1.get() + 1);
        Ok(true)
    }
    fn close(&mut self, _: CloseRequest<()>) -> CloseDecision<(), Self::Error> {
        self.0.set(self.0.get() + 1);
        self.1.set(self.1.get() + 1);
        CloseDecision::HostCleanupComplete { checkpoint: () }
    }
}
