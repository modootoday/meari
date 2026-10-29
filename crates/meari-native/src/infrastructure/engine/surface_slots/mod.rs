use super::NativeRenderer;
use crate::domain::surface_slot::*;
use crate::{
    domain::{graphics::FrameTicket, window_lifecycle::WindowStamp},
    infrastructure::native_host::HostStamp,
};
use blitz_dom::NodeId;
use std::rc::{Rc, Weak};
use style::dom::TElement;

pub(crate) trait SurfaceContext {
    fn current(&self) -> Result<(WindowStamp, u64), SurfaceError>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SurfaceRenderScope {
    pub document: HostStamp,
    pub dom_document_id: usize,
    pub window: WindowStamp,
    pub geometry_revision: u64,
    pub targets: Vec<(u64, u64)>,
}
#[derive(Clone, Debug)]
pub(crate) struct CommittedSurfacePaint {
    pub scope: SurfaceRenderScope,
    pub frame: FrameTicket,
    pub trace: blitz_paint::SurfacePaintTrace,
    pub placement: Option<RendererClientPlacement>,
}
/// Actual scene viewport, paint offset and explicitly configured renderer target, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RendererClientPlacement {
    pub viewport: (u32, u32),
    pub insets: [u32; 4], // left, top, right, bottom
    pub target: (u32, u32),
}
impl RendererClientPlacement {
    pub(crate) fn validate(self) -> Result<(), SurfaceError> {
        let width = self
            .viewport
            .0
            .checked_add(self.insets[0])
            .and_then(|v| v.checked_add(self.insets[2]));
        let height = self
            .viewport
            .1
            .checked_add(self.insets[1])
            .and_then(|v| v.checked_add(self.insets[3]));
        if self.viewport.0 == 0
            || self.viewport.1 == 0
            || width != Some(self.target.0)
            || height != Some(self.target.1)
        {
            return Err(SurfaceError::InvalidGeometry);
        }
        Ok(())
    }
}
#[derive(Default)]
pub(crate) struct SurfaceObservationState {
    context: Option<Rc<dyn SurfaceContext>>,
    targets: std::collections::BTreeMap<u64, (u64, Weak<()>)>,
    committed: Option<CommittedSurfacePaint>,
}

#[cfg(test)]
pub(crate) struct Measurement {
    pub rect: PhysicalClientRect,
    pub visibility: SurfaceVisibility,
    pub clip: SurfaceClip,
    pub plane: SurfacePlane,
}
impl NativeRenderer {
    pub(crate) fn track_surface(
        &self,
        context: Rc<dyn SurfaceContext>,
        node: u64,
        generation: u64,
        owner: Weak<()>,
    ) -> Result<(), SurfaceError> {
        let mut state = self
            .surface_observation
            .try_borrow_mut()
            .map_err(|_| SurfaceError::Busy)?;
        state
            .targets
            .retain(|_, (_, owner)| owner.strong_count() != 0);
        if state.targets.len() >= 64 && !state.targets.contains_key(&node) {
            return Err(SurfaceError::Capacity);
        }
        state.context = Some(context);
        state.targets.insert(node, (generation, owner));
        state.committed = None;
        Ok(())
    }
    pub(crate) fn invalidate_surface_paint(&self) {
        self.surface_observation.borrow_mut().committed = None;
    }
    pub(crate) fn surface_render_scope(
        &self,
        document: &HostStamp,
    ) -> Result<SurfaceRenderScope, SurfaceError> {
        let state = self
            .surface_observation
            .try_borrow()
            .map_err(|_| SurfaceError::Busy)?;
        let (window, geometry_revision) = state
            .context
            .as_ref()
            .ok_or(SurfaceError::NotReady)?
            .current()?;
        let targets = state
            .targets
            .iter()
            .filter(|(_, (_, owner))| owner.strong_count() != 0)
            .map(|(node, (generation, _))| (*node, *generation))
            .collect();
        Ok(SurfaceRenderScope {
            document: document.clone(),
            dom_document_id: self.document.id(),
            window,
            geometry_revision,
            targets,
        })
    }
    pub(crate) fn commit_surface_paint(
        &self,
        scope: SurfaceRenderScope,
        frame: FrameTicket,
        trace: blitz_paint::SurfacePaintTrace,
        placement: Option<RendererClientPlacement>,
    ) -> Result<(), SurfaceError> {
        self.invalidate_surface_paint();
        if let Some(placement) = placement {
            placement.validate()?;
        }
        if self.document.has_pending_critical_resources()
            || self.surface_render_scope(&scope.document)? != scope
            || trace.document_id != scope.dom_document_id
        {
            return Err(SurfaceError::Stale);
        }
        if scope.targets.iter().any(|(node, generation)| {
            trace
                .targets
                .get(node)
                .is_none_or(|target| target.generation != *generation)
        }) {
            return Err(SurfaceError::Stale);
        }
        self.surface_observation
            .try_borrow_mut()
            .map_err(|_| SurfaceError::Busy)?
            .committed = Some(CommittedSurfacePaint {
            scope,
            frame,
            trace,
            placement,
        });
        Ok(())
    }
    pub(crate) fn committed_surface_paint(
        &self,
        document: &HostStamp,
    ) -> Result<CommittedSurfacePaint, SurfaceError> {
        if self.document.has_pending_critical_resources() {
            return Err(SurfaceError::NotReady);
        }
        let scope = self.surface_render_scope(document)?;
        let state = self
            .surface_observation
            .try_borrow()
            .map_err(|_| SurfaceError::Busy)?;
        let committed = state.committed.as_ref().ok_or(SurfaceError::NotReady)?;
        if committed.scope != scope
            || committed.trace.focused_node
                != self
                    .document
                    .get_focussed_node_id()
                    .map(|node| node.as_u64())
        {
            return Err(SurfaceError::Stale);
        }
        Ok(committed.clone())
    }
    pub(crate) fn surface_identity(&self) -> Weak<()> {
        Rc::downgrade(&self.surface_identity)
    }
    pub(crate) fn surface_node(&self, id: &str) -> Result<NodeId, SurfaceError> {
        let node = self
            .document
            .get_element_by_id(id)
            .ok_or(SurfaceError::NotVideo)?;
        if !self
            .document
            .get_node(node)
            .is_some_and(|node| node.local_name() == "video")
        {
            return Err(SurfaceError::NotVideo);
        }
        Ok(node)
    }
    #[cfg(test)]
    pub(crate) fn measure_surface(
        &self,
        id: &str,
        expected: NodeId,
    ) -> Result<Measurement, SurfaceError> {
        if self.surface_node(id)? != expected {
            return Err(SurfaceError::Stale);
        }
        let bounds = self
            .document
            .get_client_bounding_rect(expected)
            .ok_or(SurfaceError::NotReady)?;
        // Blitz's client-box API returns physical unrounded layout coordinates. Paint transforms
        // and effective clipping/stacking are separate, so this does not certify an OS plane.
        let rect = PhysicalClientRect {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
        };
        rect.validate()?;
        let mut visibility = if rect.width == 0.0 || rect.height == 0.0 {
            SurfaceVisibility::Hidden
        } else {
            // A nonzero box alone does not establish effective clipping/occlusion visibility.
            SurfaceVisibility::Unknown
        };
        let mut unsupported = false;
        let mut cursor = Some(expected);
        let mut visited = std::collections::BTreeSet::new();
        while let Some(id) = cursor {
            if !visited.insert(id) || visited.len() > 256 {
                return Err(SurfaceError::Unsupported("ancestor depth or cycle"));
            }
            let node = self.document.get_node(id).ok_or(SurfaceError::Stale)?;
            if let Some(transform) = node.transform().as_deref()
                && transform.as_coeffs() != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0]
            {
                unsupported = true;
            }
            if node.is_element() {
                let Some(styles) = node.primary_styles() else {
                    return Err(SurfaceError::NotReady);
                };
                if (id == expected
                    && styles.get_inherited_box().visibility
                        != style::computed_values::visibility::T::Visible)
                    || styles.get_effects().opacity == 0.0
                {
                    visibility = SurfaceVisibility::Hidden;
                }
                if styles.get_effects().opacity != 1.0 {
                    unsupported = true;
                }
            }
            cursor = node.parent;
        }
        Ok(Measurement {
            rect,
            visibility,
            clip: if unsupported {
                SurfaceClip::Unsupported
            } else {
                SurfaceClip::Unknown
            },
            plane: if unsupported {
                SurfacePlane::Unsupported
            } else {
                SurfacePlane::Unknown
            },
        })
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
