//! Split from the parent module to keep files under the 600-line structure limit.
use super::super::*;
use crate::infrastructure::{
    accessibility,
    native_host::{self, access},
};
use accesskit_xplat::{Adapter, WindowEvent as AccessibilityEvent};
use std::sync::Arc;
use winit::{event_loop::ActiveEventLoop, raw_window_handle::HasWindowHandle, window::WindowId};

impl Application {
    pub(in super::super) fn refresh_host(&mut self, id: WindowId) {
        let Some(view) = self.inner.windows.get_mut(&id) else {
            return;
        };
        if view.window.surface_size().width == 0 || view.window.surface_size().height == 0 {
            return;
        }
        let result = access::with_mut(view.doc.as_mut(), |document| {
            let zoom = f64::from(document.renderer.document.get_viewport().zoom);
            let result = super::super::super::host::snapshot_with_zoom(view.window.as_ref(), zoom)
                .and_then(|environment| document.commit_host_environment(environment));
            if let Err(error) = &result {
                document.last_error = Some(error.clone());
            }
            result
        })
        .map_err(|error| error.to_string())
        .and_then(|result| result);
        if let Err(error) = result {
            self.errors
                .lock()
                .expect("native diagnostics lock")
                .push(error);
        }
        // A separate observation after the mutable document guard has been released.
        let observation = self.inner.windows.get(&id).and_then(|view| {
            let handle = access::handle(view.doc.as_ref())?;
            let zoom = handle
                .try_with(|document| f64::from(document.renderer.document.get_viewport().zoom));
            Some((
                handle.id(),
                zoom.map_err(|error| error.to_string()).and_then(|zoom| {
                    super::super::super::host::snapshot_with_zoom(view.window.as_ref(), zoom)
                }),
            ))
        });
        if let Some((host, environment)) = observation {
            let result = environment.and_then(|environment| {
                self.lifecycles
                    .get(&host)
                    .map_or(Ok(()), |lifecycle| lifecycle.geometry(environment))
            });
            if let Err(error) = result {
                self.report(error);
            }
        }
    }

    pub(in super::super) fn lifecycle_turn(&mut self, event_loop: &dyn ActiveEventLoop, host: u64) {
        if let Some(lifecycle) = self.lifecycles.get(&host)
            && let Err(error) = lifecycle.poll()
        {
            self.report(error);
        }
        // Completion may continue Pending, but never automatically retry a failed business/child close.
        if self.close_observation_pending(host) {
            self.observe_close(event_loop, host, native_host::CloseCause::Host, false);
        }
    }

    pub(in super::super) fn close_observation_pending(&self, host: u64) -> bool {
        self.hosts
            .get(&host)
            .is_some_and(|port| port.document().lifecycle() == native_host::HostLifecycle::Closing)
            || self
                .closes
                .get(&host)
                .is_some_and(|schedule| schedule.dirty && !schedule.failed)
    }

    pub(in super::super) fn creation_failed(&self, host: u64, error: String) {
        self.report(error);
        if let Some(lifecycle) = self.lifecycles.get(&host)
            && let Err(error) = lifecycle.failed()
        {
            self.report(error);
        }
    }

    pub(in super::super) fn resume_window(&mut self, id: WindowId) {
        let host = self
            .inner
            .windows
            .get(&id)
            .and_then(|view| access::handle(view.doc.as_ref()))
            .map(|handle| handle.id());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(
            || -> Result<Option<Adapter>, String> {
                let view = self
                    .inner
                    .windows
                    .get_mut(&id)
                    .ok_or("native view disappeared")?;
                // AccessKit subclasses the native window once, before its first visibility.
                // Surface suspension does not end that window's accessibility lifetime.
                let adapter = if self.adapters.contains_key(&id) {
                    None
                } else {
                    let handle = view
                        .window
                        .window_handle()
                        .map_err(|error| error.to_string())?;
                    Some(Adapter::with_combined_handler(
                        handle.as_raw(),
                        Arc::new(Handler {
                            window: id,
                            proxy: self.inner.proxy.clone(),
                        }),
                    ))
                };
                view.resume();
                view.window.set_visible(true);
                view.is_visible = true;
                view.window.request_redraw();
                Ok(adapter)
            },
        ))
        .unwrap_or_else(|_| {
            Err(
                "native accessibility/renderer initialization panicked; partial owner retained"
                    .into(),
            )
        });
        match result {
            Ok(adapter) => {
                if let Some(adapter) = adapter {
                    self.adapters.insert(id, adapter);
                }
                self.refresh_host(id);
            }
            Err(error) => {
                if let Some(host) = host {
                    self.creation_failed(host, error);
                } else {
                    self.report(error);
                }
            }
        }
    }

    pub(in super::super) fn update(&mut self, window: WindowId) {
        let Some(view) = self.inner.windows.get(&window) else {
            return;
        };
        if let Some(adapter) = self.adapters.get_mut(&window) {
            let _ = access::with(view.doc.as_ref(), |document| {
                adapter.update_if_active(|| accessibility::tree(document))
            });
        }
    }

    pub(in super::super) fn accessibility(&mut self, event: &A11yEvent) {
        let mut dispatched = false;
        if let AccessibilityEvent::ActionRequested(request) = &event.event
            && let Some(view) = self.inner.windows.get_mut(&event.window)
        {
            if access::handle(view.doc.as_ref()).is_some_and(|handle| !handle.can_dispatch_input())
            {
                return;
            }
            let result = match access::handle(view.doc.as_ref()) {
                Some(handle) => handle
                    .with_native_input(|document| accessibility::act(document, request))
                    .map_err(|error| error.to_string()),
                None => access::with_mut(view.doc.as_mut(), |document| {
                    accessibility::act(document, request)
                })
                .map_err(|error| error.to_string())
                .and_then(|result| result),
            };
            if let Err(error) = result {
                let error = format!(
                    "accessibility {:?} target {:?}: {error}",
                    request.action, request.target_node
                );
                eprintln!("{error}");
                self.errors
                    .lock()
                    .expect("native diagnostics lock")
                    .push(error);
            } else if let Some(handle) = access::handle(view.doc.as_ref()) {
                if let Err(error) = handle.note_native_action() {
                    eprintln!("native host action: {error}");
                } else {
                    dispatched = true;
                }
            }
            view.window.request_redraw();
            if request.action == accesskit::Action::Focus {
                view.window.focus_window();
            }
        }
        if dispatched {
            self.files(event.window);
            self.after_dispatch(event.window, native_host::HostTurnReason::Accessibility);
        }
        self.update(event.window);
    }

    pub(in super::super) fn files(&mut self, id: WindowId) {
        if let Some(view) = self.inner.windows.get_mut(&id) {
            let result = access::with_mut(view.doc.as_mut(), |document| {
                if document.file_requests.is_empty() {
                    return Ok(false);
                }
                document.process_file_requests(
                    &mut super::super::super::file_picker::NativeFilePicker,
                )?;
                Ok::<_, String>(true)
            })
            .map_err(|error| error.to_string())
            .and_then(|result| result);
            if let Err(error) = &result {
                self.errors
                    .lock()
                    .expect("native diagnostics lock")
                    .push(error.clone());
            }
            if matches!(result, Ok(true)) {
                view.window.request_redraw();
            }
        }
    }
}
