//! One accessor for raw documents and the managed facade, including checked rendering and a11y.
use super::{HostAccessError, NativeDocumentHandle};
use crate::infrastructure::document::InteractiveDocument;
use blitz_dom::{DocGuard, DocGuardMut, Document};
use std::{
    any::Any,
    cell::{Ref, RefMut},
};

struct SharedDocument {
    handle: NativeDocumentHandle,
}
impl Document for SharedDocument {
    fn inner(&self) -> DocGuard<'_> {
        DocGuard::RefCell(Ref::map(self.handle.0.document.borrow(), |document| {
            &*document
                .as_ref()
                .expect("window driver checked document lifetime")
                .renderer
                .document
        }))
    }
    fn inner_mut(&mut self) -> DocGuardMut<'_> {
        DocGuardMut::RefCell(RefMut::map(
            self.handle.0.document.borrow_mut(),
            |document| {
                &mut *document
                    .as_mut()
                    .expect("window driver checked document lifetime")
                    .renderer
                    .document
            },
        ))
    }
    fn handle_ui_event(&mut self, event: blitz_traits::events::UiEvent) {
        if let Err(error) = self.handle.dispatch_ui_event(event) {
            let _ = self
                .handle
                .with_internal_mut(|document| document.last_error = Some(error.to_string()));
        }
    }
}

pub(crate) fn facade(handle: NativeDocumentHandle) -> Box<dyn Document> {
    Box::new(SharedDocument { handle })
}
pub(crate) fn handle(document: &dyn Document) -> Option<NativeDocumentHandle> {
    (document as &dyn Any)
        .downcast_ref::<SharedDocument>()
        .map(|document| document.handle.clone())
}
pub(crate) fn with<R>(
    document: &dyn Document,
    visit: impl FnOnce(&InteractiveDocument) -> R,
) -> Result<R, HostAccessError> {
    if let Some(document) = (document as &dyn Any).downcast_ref::<InteractiveDocument>() {
        return Ok(visit(document));
    }
    handle(document)
        .ok_or_else(|| {
            HostAccessError::Invalid("native document adapter is not registered".into())
        })?
        .try_with(visit)
}
pub(crate) fn with_mut<R>(
    document: &mut dyn Document,
    visit: impl FnOnce(&mut InteractiveDocument) -> R,
) -> Result<R, HostAccessError> {
    if (document as &dyn Any).is::<InteractiveDocument>() {
        return Ok(visit(
            (document as &mut dyn Any)
                .downcast_mut::<InteractiveDocument>()
                .expect("native document type checked"),
        ));
    }
    handle(document)
        .ok_or_else(|| {
            HostAccessError::Invalid("native document adapter is not registered".into())
        })?
        .with_internal_mut(visit)
}
