use super::*;
use meari::application::host_callbacks::{CallbackHandle, PendingCallback};
use wasm_bindgen::closure::Closure;
use web_sys::{EventTarget, HtmlInputElement};

const NAVIGATION: &str = include_str!("../../../fixtures/node-slots/studio/navigation-item.tsx");

#[wasm_bindgen_test]
fn browser_original_navigation_capture_updates_atomically_and_revokes_old_export() {
    let (mut document, mut host, container, paths) = mount(NAVIGATION, "browser-prop-capture");
    let button: HtmlElement = document
        .element(&paths["onNavigate"])
        .unwrap()
        .unchecked_into();
    assert_eq!(button.get_attribute("aria-label").as_deref(), Some("value"));
    button.click();
    assert!(
        document.last_error().is_none(),
        "{:?}",
        document.last_error()
    );
    assert!(host.requests.borrow().is_empty());
    let first = document.drain_callbacks().pop().unwrap();
    assert_eq!(
        first.invocation.arguments,
        CallbackArguments::Text("value".into())
    );
    assert_eq!(
        first.invocation.span,
        meari::domain::Span {
            start: 349,
            end: 390
        }
    );
    let mut values = props(NAVIGATION);
    values.insert("workspaceId".into(), Value::String("다음 작업/🌊".into()));
    document
        .update_module_props_with_callbacks(values.clone(), host.owner.export_bindings().unwrap())
        .unwrap();
    assert!(button.is_same_node(Some(
        document.element(&paths["onNavigate"]).unwrap().as_ref()
    )));
    assert_eq!(
        host.owner
            .dispatch(
                &first.handle,
                first.invocation.arguments,
                &mut host.registry
            )
            .unwrap_err()
            .code,
        PluginErrorCode::Stale
    );
    button.click();
    let current = document.drain_callbacks().pop().unwrap();
    assert_eq!(
        current.invocation.arguments,
        CallbackArguments::Text("다음 작업/🌊".into())
    );
    values.insert("workspaceId".into(), Value::Bool(false));
    assert!(
        document
            .update_module_props_with_callbacks(values, host.owner.export_bindings().unwrap())
            .is_err()
    );
    host.owner
        .dispatch(
            &current.handle,
            current.invocation.arguments,
            &mut host.registry,
        )
        .unwrap();
    button.click();
    let after_failure = document.drain_callbacks().pop().unwrap();
    assert_eq!(
        after_failure.invocation.arguments,
        CallbackArguments::Text("다음 작업/🌊".into())
    );
    button.set_attribute("disabled", "").unwrap();
    button.click();
    assert!(document.drain_callbacks().is_empty());
    button.remove_attribute("disabled").unwrap();
    button.set_attribute("hidden", "").unwrap();
    button.click();
    assert!(document.drain_callbacks().is_empty());
    assert_eq!(host.requests.borrow().len(), 1);
    document.close();
    container.remove();
}

thread_local! {
    static LIVE_CAPTURE: RefCell<Option<Rc<RefCell<LiveCapture>>>> = const { RefCell::new(None) };
}

struct StoryListener {
    target: EventTarget,
    callback: Closure<dyn FnMut(Event)>,
}
impl StoryListener {
    fn new(target: EventTarget, callback: impl FnMut(Event) + 'static) -> Self {
        let callback = Closure::wrap(Box::new(callback) as Box<dyn FnMut(Event)>);
        target
            .add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())
            .unwrap();
        Self { target, callback }
    }
    fn detach(&self) {
        self.target
            .remove_event_listener_with_callback("click", self.callback.as_ref().unchecked_ref())
            .unwrap();
    }
}
impl Drop for StoryListener {
    fn drop(&mut self) {
        self.detach();
    }
}

struct LiveCapture {
    document: WebDocument,
    scope: meari::domain::plugins::PluginScope,
    host: Host,
    container: Element,
    button: HtmlElement,
    accept_button: Element,
    input: HtmlInputElement,
    status: Element,
    values: BTreeMap<String, Value>,
    last_export: Option<(CallbackHandle, CallbackArguments)>,
    pending: Option<(bool, String, usize)>,
    deferred: Option<Closure<dyn FnMut()>>,
    scheduled: Option<i32>,
    listeners: Vec<StoryListener>,
    observations: Vec<serde_json::Value>,
    dispatched: usize,
    closed: bool,
}
impl LiveCapture {
    fn report(&self) {
        self.status.set_text_content(Some(
            &serde_json::json!({
                "accepted": self.values["workspaceId"],
                "registryCount": self.host.requests.borrow().len(),
                "observations": self.observations,
                "closed": self.closed,
                "generation": self.scope.source_generation,
            })
            .to_string(),
        ));
    }
    fn drain(&mut self) {
        self.scheduled = None;
        if self.closed {
            return;
        }
        assert_eq!(self.document.callback_scope().as_ref(), Some(&self.scope));
        let Some((trusted, current_target_text, before)) = self.pending.take() else {
            return;
        };
        let deferred_registry = self.host.requests.borrow().len() == before;
        let pending = self.document.drain_callbacks();
        assert_eq!(pending.len(), 1);
        let PendingCallback { handle, invocation } = pending.into_iter().next().unwrap();
        assert_eq!(
            invocation.span,
            meari::domain::Span {
                start: 349,
                end: 390
            }
        );
        let captured = match &invocation.arguments {
            CallbackArguments::Text(value) => value.clone(),
            _ => panic!("original NavigationItem must capture a string"),
        };
        assert_eq!(self.values["workspaceId"], Value::String(captured.clone()));
        assert_ne!(captured, current_target_text);
        self.host
            .owner
            .dispatch(
                &handle,
                invocation.arguments.clone(),
                &mut self.host.registry,
            )
            .unwrap();
        self.dispatched += 1;
        assert_eq!(self.host.requests.borrow().len(), self.dispatched);
        self.last_export = Some((handle, invocation.arguments));
        self.accept_button.remove_attribute("disabled").unwrap();
        let focused = self
            .button
            .owner_document()
            .unwrap()
            .active_element()
            .is_some_and(|active| active.is_same_node(Some(self.button.as_ref())));
        self.observations.push(serde_json::json!({
            "trusted": trusted, "captured": captured,
            "currentTargetText": current_target_text,
            "registryDeferred": deferred_registry, "focused": focused,
        }));
        self.report();
    }
    fn accept(&mut self) {
        assert!(
            self.pending.is_none(),
            "accept follows the deferred host turn"
        );
        assert!(
            self.observations.len() < 64,
            "live story observation budget"
        );
        let next = self.input.value();
        assert_ne!(next, self.button.text_content().unwrap_or_default());
        let mut values = self.values.clone();
        values.insert("workspaceId".into(), Value::String(next.clone()));
        self.document
            .update_module_props_with_callbacks(
                values.clone(),
                self.host.owner.export_bindings().unwrap(),
            )
            .unwrap();
        self.values = values;
        let before = self.host.requests.borrow().len();
        let stale = self.last_export.as_ref().map(|(handle, arguments)| {
            self.host
                .owner
                .dispatch(handle, arguments.clone(), &mut self.host.registry)
                .unwrap_err()
                .code
        });
        assert_eq!(stale, Some(PluginErrorCode::Stale));
        assert_eq!(self.host.requests.borrow().len(), before);
        self.observations.push(serde_json::json!({
            "acceptedUpdate": next, "oldExportRejected": true,
        }));
        self.report();
    }
    fn close(&mut self) {
        if self.closed {
            return;
        }
        self.closed = true;
        for listener in &self.listeners {
            listener.detach();
        }
        self.pending = None;
        if let Some(handle) = self.scheduled.take() {
            web_sys::window().unwrap().clear_timeout_with_handle(handle);
        }
        self.document.close();
        self.report();
        self.container.remove();
    }
}
impl Drop for LiveCapture {
    fn drop(&mut self) {
        self.close();
    }
}

/// Leaves a Rust-owned fixture for subsequent IAB input; mounting is not trusted-input proof.
#[wasm_bindgen_test]
fn browser_live_original_capture_keyboard_story() {
    let entry = BuildPath::try_from("navigation-item.tsx".to_owned()).unwrap();
    let graph = meari::parse_module_graph(
        &entry,
        &BTreeMap::from([(entry.clone(), NAVIGATION.into())]),
    )
    .unwrap();
    let mut values = props(NAVIGATION);
    values.insert(
        "workspaceId".into(),
        Value::String("workspace/initial".into()),
    );
    values.insert(
        "label".into(),
        Value::String("Live capture target label".into()),
    );
    let program = ModuleProgram::mount(graph, values.clone(), "live-capture".into(), 1).unwrap();
    assert!(
        program
            .project()
            .unwrap()
            .callback_bindings()
            .values()
            .all(|handler| {
                handler.source == meari::domain::host_callbacks::CallbackSource::CapturedText
            })
    );
    let (document, host, container, paths) = mount_program(program, "live-capture");
    let scope = document.callback_scope().unwrap();
    container
        .set_attribute("aria-label", "Meari live capture story")
        .unwrap();
    let browser = container.owner_document().unwrap();
    let button = document
        .element(&paths["onNavigate"])
        .unwrap()
        .clone()
        .unchecked_into();
    let input: HtmlInputElement = browser.create_element("input").unwrap().unchecked_into();
    input
        .set_attribute("aria-label", "Next accepted workspace ID")
        .unwrap();
    input.set_value("workspace/accepted");
    container.append_child(&input).unwrap();
    let accept = browser.create_element("button").unwrap();
    accept.set_text_content(Some("Accept workspace ID"));
    accept.set_attribute("disabled", "").unwrap();
    container.append_child(&accept).unwrap();
    let close = browser.create_element("button").unwrap();
    close.set_text_content(Some("Close live capture story"));
    container.append_child(&close).unwrap();
    let status = browser.create_element("pre").unwrap();
    status.set_attribute("role", "status").unwrap();
    status
        .set_attribute("aria-label", "Live capture observations")
        .unwrap();
    container.append_child(&status).unwrap();
    let story = Rc::new(RefCell::new(LiveCapture {
        document,
        scope,
        host,
        container,
        button,
        accept_button: accept.clone(),
        input,
        status,
        values,
        last_export: None,
        pending: None,
        deferred: None,
        scheduled: None,
        listeners: Vec::new(),
        observations: Vec::new(),
        dispatched: 0,
        closed: false,
    }));
    let weak = Rc::downgrade(&story);
    story.borrow_mut().deferred = Some(Closure::wrap(Box::new(move || {
        if let Some(story) = weak.upgrade() {
            story.borrow_mut().drain();
        }
    }) as Box<dyn FnMut()>));
    let weak = Rc::downgrade(&story);
    let listener = StoryListener::new(story.borrow().button.clone().into(), move |event| {
        let Some(story) = weak.upgrade() else { return };
        let mut story = story.borrow_mut();
        if story.closed {
            return;
        }
        assert!(story.pending.is_none());
        assert!(
            story.observations.len() < 64,
            "live story observation budget"
        );
        let before = story.host.requests.borrow().len();
        assert_eq!(
            before, story.dispatched,
            "registry must not run inside the event"
        );
        let current_target_text = event
            .current_target()
            .unwrap()
            .dyn_into::<HtmlElement>()
            .unwrap()
            .text_content()
            .unwrap_or_default();
        story.pending = Some((event.is_trusted(), current_target_text, before));
        // A new host turn avoids microtask checkpoints between native event listeners.
        story.scheduled = Some(
            web_sys::window()
                .unwrap()
                .set_timeout_with_callback_and_timeout_and_arguments_0(
                    story.deferred.as_ref().unwrap().as_ref().unchecked_ref(),
                    0,
                )
                .unwrap(),
        );
    });
    story.borrow_mut().listeners.push(listener);
    let weak = Rc::downgrade(&story);
    let listener = StoryListener::new(accept.into(), move |_| {
        if let Some(story) = weak.upgrade() {
            let mut story = story.borrow_mut();
            if !story.closed {
                story.accept();
            }
        }
    });
    story.borrow_mut().listeners.push(listener);
    let weak = Rc::downgrade(&story);
    let listener = StoryListener::new(close.into(), move |_| {
        if let Some(story) = weak.upgrade() {
            story.borrow_mut().close();
            let closed = Rc::downgrade(&story);
            let release = Closure::once_into_js(move || {
                LIVE_CAPTURE.with_borrow_mut(|slot| {
                    if slot
                        .as_ref()
                        .zip(closed.upgrade())
                        .is_some_and(|(current, closed)| Rc::ptr_eq(current, &closed))
                    {
                        *slot = None;
                    }
                });
            });
            // Release the owner after the currently executing listener has returned.
            web_sys::window()
                .unwrap()
                .queue_microtask(release.unchecked_ref());
        }
    });
    story.borrow_mut().listeners.push(listener);
    story.borrow().report();
    LIVE_CAPTURE.with_borrow_mut(|slot| *slot = Some(story));
}
