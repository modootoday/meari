//! Winit/softbuffer surface with our AccessKit action handler, rather than the upstream TODO.
use crate::infrastructure::{accessibility, document::InteractiveDocument};
use accesskit_xplat::{Adapter, EventHandler, WindowEvent as AccessibilityEvent};
use anyrender_vello_cpu::VelloCpuWindowRenderer;
use blitz_shell::{BlitzApplication, BlitzShellEvent, BlitzShellProxy, WindowConfig};
use std::{any::Any, collections::HashMap, sync::Arc, time::Duration};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::ActiveEventLoop,
    raw_window_handle::HasWindowHandle,
    window::{WindowAttributes, WindowId},
};

struct A11yEvent {
    window: WindowId,
    event: AccessibilityEvent,
}
struct Handler {
    window: WindowId,
    proxy: BlitzShellProxy,
}
impl EventHandler for Handler {
    fn handle_accesskit_event(&self, event: AccessibilityEvent) {
        self.proxy
            .send_event(BlitzShellEvent::embedder_event(A11yEvent {
                window: self.window,
                event,
            }));
    }
}
struct Stop;
struct Application {
    inner: BlitzApplication<VelloCpuWindowRenderer>,
    adapters: HashMap<WindowId, Adapter>,
    errors: Arc<std::sync::Mutex<Vec<String>>>,
}

impl Application {
    fn update(&mut self, window: WindowId) {
        let Some(view) = self.inner.windows.get(&window) else {
            return;
        };
        let Some(document) = (&*view.doc as &dyn Any).downcast_ref::<InteractiveDocument>() else {
            return;
        };
        if let Some(adapter) = self.adapters.get_mut(&window) {
            adapter.update_if_active(|| accessibility::tree(document));
        }
    }
    fn accessibility(&mut self, event: &A11yEvent) {
        if let AccessibilityEvent::ActionRequested(request) = &event.event
            && let Some(view) = self.inner.windows.get_mut(&event.window)
            && let Some(document) =
                (&mut *view.doc as &mut dyn Any).downcast_mut::<InteractiveDocument>()
        {
            if let Err(error) = accessibility::act(document, request) {
                let error = format!(
                    "accessibility {:?} target {:?}: {error}",
                    request.action, request.target_node
                );
                eprintln!("{error}");
                self.errors
                    .lock()
                    .expect("native diagnostics lock")
                    .push(error);
            }
            view.window.request_redraw();
            if request.action == accesskit::Action::Focus {
                view.window.focus_window();
            }
        }
        self.files(event.window);
        self.update(event.window);
    }
    fn files(&mut self, id: WindowId) {
        if let Some(view) = self.inner.windows.get_mut(&id)
            && let Some(document) =
                (&mut *view.doc as &mut dyn Any).downcast_mut::<InteractiveDocument>()
            && !document.file_requests.is_empty()
        {
            if let Err(error) =
                document.process_file_requests(&mut super::file_picker::NativeFilePicker)
            {
                self.errors
                    .lock()
                    .expect("native diagnostics lock")
                    .push(error);
            }
            view.window.request_redraw();
        }
    }
}

impl ApplicationHandler for Application {
    fn can_create_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        for view in self.inner.windows.values_mut() {
            view.resume();
        }
        for config in self.inner.pending_windows.drain(..) {
            let mut view = blitz_shell::View::init(config, event_loop, &self.inner.proxy);
            let id = view.window_id();
            let adapter = Adapter::with_combined_handler(
                view.window
                    .window_handle()
                    .expect("native window handle")
                    .as_raw(),
                Arc::new(Handler {
                    window: id,
                    proxy: self.inner.proxy.clone(),
                }),
            );
            self.adapters.insert(id, adapter);
            view.resume();
            view.window.set_visible(true);
            view.is_visible = true;
            view.window.request_redraw();
            self.inner.windows.insert(id, view);
        }
    }
    fn destroy_surfaces(&mut self, event_loop: &dyn ActiveEventLoop) {
        self.adapters.clear();
        self.inner.destroy_surfaces(event_loop);
    }
    fn window_event(&mut self, event_loop: &dyn ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if let Some(adapter) = self.adapters.get_mut(&id) {
            if let WindowEvent::Focused(focused) = &event {
                adapter.set_focus(*focused);
            }
            // Viewport changes are projected on the next redraw. OS bounds are updated below.
            if let Some(view) = self.inner.windows.get(&id) {
                let position = view
                    .window
                    .outer_position()
                    .unwrap_or_default()
                    .cast::<f64>();
                let outer = view.window.outer_size().cast::<f64>();
                let inner_position = view.window.surface_position().cast::<f64>();
                let inner = view.window.surface_size().cast::<f64>();
                adapter.set_window_bounds(
                    accesskit::Rect::from_origin_size(
                        (position.x, position.y),
                        (outer.width, outer.height),
                    ),
                    accesskit::Rect::from_origin_size(
                        (inner_position.x, inner_position.y),
                        (inner.width, inner.height),
                    ),
                );
            }
        }
        let close = matches!(event, WindowEvent::CloseRequested);
        if close {
            self.adapters.remove(&id);
        }
        self.inner.window_event(event_loop, id, event);
        if !close {
            self.files(id);
            self.update(id);
        }
    }
    fn proxy_wake_up(&mut self, event_loop: &dyn ActiveEventLoop) {
        while let Ok(event) = self.inner.event_queue.try_recv() {
            if let BlitzShellEvent::Embedder(data) = &event {
                if data.is::<Stop>() {
                    self.adapters.clear();
                    self.inner.windows.clear();
                    event_loop.exit();
                    return;
                }
                if let Some(event) = data.downcast_ref::<A11yEvent>() {
                    self.accessibility(event);
                    continue;
                }
            }
            self.inner.handle_blitz_shell_event(event_loop, event);
        }
        for id in self.inner.windows.keys().copied().collect::<Vec<_>>() {
            self.update(id);
        }
    }
}

pub fn show(document: InteractiveDocument, seconds: Option<u64>) -> Result<(), String> {
    println!("native_window_pid={}", std::process::id());
    let event_loop = blitz_shell::create_default_event_loop();
    let (proxy, events) = BlitzShellProxy::new(event_loop.create_proxy());
    let mut inner = BlitzApplication::new(proxy.clone(), events);
    let renderer = VelloCpuWindowRenderer::new();
    inner.add_window(WindowConfig::with_attributes(
        Box::new(document),
        renderer,
        WindowAttributes::default()
            .with_title("Meari Native")
            .with_visible(false),
    ));
    if let Some(seconds) = seconds {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(seconds.clamp(1, 600)));
            proxy.send_event(BlitzShellEvent::embedder_event(Stop));
        });
    }
    let errors = Arc::new(std::sync::Mutex::new(Vec::new()));
    let app = Application {
        inner,
        adapters: HashMap::new(),
        errors: errors.clone(),
    };
    event_loop.run_app(app).map_err(|e| e.to_string())?;
    let errors = errors.lock().map_err(|e| e.to_string())?;
    if !errors.is_empty() {
        return Err(errors.join("; "));
    }
    Ok(())
}
