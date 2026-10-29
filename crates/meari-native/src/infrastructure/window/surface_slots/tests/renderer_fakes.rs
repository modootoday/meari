//! Split from the parent module to keep files under the 600-line structure limit.
use super::*;
use crate::{
    domain::Viewport,
    infrastructure::window::{forwarding::ForwardingSeed, parent::ParentBinding},
};
use std::{io, task::Waker, time::Duration};

pub(super) fn actual_unknown_ink_renderer(
    region: &str,
) -> crate::infrastructure::engine::NativeRenderer {
    painted_renderer(&format!(
        "<style>body{{margin:0;font:16px sans-serif}}video{{display:block;width:120px;height:60px}}.region{{margin-top:12px;width:240px;height:80px;{region}}}p{{margin:0}}button:disabled{{display:block;height:20px;font-size:12px;opacity:0.5}}.shadow{{width:60px;height:10px;box-shadow:0 0 5px red}}</style><video id='surface'></video><div class='region'><p>Real guide text</p><button disabled>Disabled text</button><div class='shadow'></div><svg width='24' height='24'><path d='M2 2L20 20' stroke='black' stroke-width='2' fill='none'/></svg></div>"
    ))
}

impl MonotonicClock for Clock {
    fn now(&self) -> Duration {
        Duration::ZERO
    }
}

impl NativeChild for Child {
    type Error = io::Error;
    fn parent_lost(&mut self) {}
    fn begin_cleanup(
        &mut self,
        _: CleanupRequest,
        _: CompletionSignal,
    ) -> Result<CleanupProgress, io::Error> {
        Ok(CleanupProgress::Complete)
    }
    fn observe_cleanup(&mut self, _: CleanupRequest) -> Result<CleanupProgress, io::Error> {
        Ok(CleanupProgress::Complete)
    }
}

pub(super) fn fixture() -> (NativeSurfaceRegistrar<Child>, String, String) {
    let source = meari::parse(
        "type Props = {}; export function Surface(props: Props) { return <div><video /><div /></div>; }",
        meari::application::Language::Tsx,
    )
    .unwrap();
    let document = InteractiveDocument::new(
        source,
        "video { display:block; width:120px; height:60px; }",
        Viewport {
            width: 320,
            height: 180,
            dark: false,
        },
    )
    .unwrap();
    let video = document
        .elements()
        .iter()
        .find(|element| element.dom_tag == "video")
        .unwrap()
        .id
        .clone();
    let other = document
        .elements()
        .iter()
        .find(|element| element.dom_tag == "div")
        .unwrap()
        .id
        .clone();
    let document = NativeDocumentHandle::new(document, 1).unwrap();
    let seed = ForwardingSeed::new(document.id());
    let coordinator = Rc::new(
        WindowLifecycle::new(
            LifecycleLimits::new(4, Duration::from_secs(1)).unwrap(),
            Clock,
            Waker::noop().clone(),
        )
        .unwrap(),
    );
    coordinator.ready(ReadyKind::NewWindow).unwrap();
    let parent = NativeChildRegistrar {
        binding: ParentBinding::new(document.id(), seed, None),
        coordinator,
    };
    (NativeSurfaceRegistrar::new(document, parent), video, other)
}

impl ParentAccess for RejectAccess {
    fn visit(
        &self,
        request: Option<CleanupRequest>,
        _: &mut dyn FnMut(&NativeScopedParent<'_>),
    ) -> Result<(), LifecycleError> {
        *self.0.borrow_mut() = request;
        Err(LifecycleError::Stale)
    }
}

pub(super) fn painted_renderer(html: &str) -> crate::infrastructure::engine::NativeRenderer {
    use crate::application::Renderer;
    let mut renderer = crate::infrastructure::engine::NativeRenderer::from_html(html);
    renderer
        .layout(
            &Viewport {
                width: 320,
                height: 180,
                dark: false,
            },
            0.0,
        )
        .unwrap();
    renderer
}

impl crate::infrastructure::engine::surface_slots::SurfaceContext for PaintContext {
    fn current(&self) -> Result<(WindowStamp, u64), SurfaceError> {
        Ok((self.window, self.revision.get()))
    }
}
