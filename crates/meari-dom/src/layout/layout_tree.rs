use super::*;

/// The default object size for replaced elements
/// (https://drafts.csswg.org/css-images/#default-object-size).
pub(super) const DEFAULT_OBJECT_SIZE: taffy::Size<f32> = taffy::Size {
    width: 300.0,
    height: 150.0,
};

/// The intrinsic dimensions and default object size for a replaced element
/// whose intrinsic dimensions are determined by its tag: an image with no
/// loaded resource has no intrinsic dimensions and a zero default object size;
/// a canvas has an intrinsic size and aspect ratio given by its width/height
/// attributes (defaulting to 300x150); other replaced elements (video, iframe,
/// embed) have no intrinsic dimensions and the 300x150 default object size.
pub(super) fn tag_intrinsic_sizes(
    tag_name: &LocalName,
    attr_size: taffy::Size<Option<f32>>,
) -> (IntrinsicSizes, taffy::Size<f32>) {
    if *tag_name == local_name!("img") || *tag_name == local_name!("svg") {
        return (IntrinsicSizes::default(), taffy::Size::ZERO);
    }
    if *tag_name == local_name!("canvas") {
        let width = attr_size.width.unwrap_or(300.0);
        let height = attr_size.height.unwrap_or(150.0);
        return (
            IntrinsicSizes {
                width: Some(width),
                height: Some(height),
                ratio: Some(width / height),
            },
            DEFAULT_OBJECT_SIZE,
        );
    }
    (IntrinsicSizes::default(), DEFAULT_OBJECT_SIZE)
}

pub(crate) fn resolve_calc_value(calc_ptr: *const (), parent_size: f32) -> f32 {
    let calc = unsafe { &*(calc_ptr as *const CalcLengthPercentage) };
    let result = calc.resolve(CSSPixelLength::new(parent_size));
    result.px()
}

impl<'doc> LayoutPassState<'doc> {
    /// The containing block's inline size when `dom_id` is the box currently laid out in an orthogonal
    /// flow: its percentage padding resolves against it (see `layout::writing_mode`).
    #[inline]
    pub(super) fn orthogonal_percent_basis_of(&self, dom_id: crate::NodeId) -> Option<f32> {
        #[cfg(feature = "writing-mode")]
        {
            self.orthogonal_percent_basis
                .filter(|(node, _)| *node == dom_id)
                .map(|(_, basis)| basis)
        }
        #[cfg(not(feature = "writing-mode"))]
        {
            let _ = dom_id;
            None
        }
    }

    pub(crate) fn new(doc: &'doc mut BaseDocument) -> Self {
        #[cfg(feature = "writing-mode")]
        let root_id = doc.try_root_element().map(|root| root.id);
        Self {
            #[cfg(feature = "writing-mode")]
            root_wm: root_id.map_or(stylo_taffy::WritingMode::empty(), |id| {
                doc.root_layout_wm(id)
            }),
            doc,
            #[cfg(feature = "writing-mode")]
            root_id,
            #[cfg(feature = "writing-mode")]
            layout_wm: stylo_taffy::WritingMode::empty(),
            #[cfg(feature = "writing-mode")]
            current_align_axis_is_inline: false,
            #[cfg(feature = "writing-mode")]
            orthogonal_percent_basis: None,
        }
    }
}

impl Deref for LayoutPassState<'_> {
    type Target = BaseDocument;
    fn deref(&self) -> &BaseDocument {
        self.doc
    }
}

impl DerefMut for LayoutPassState<'_> {
    fn deref_mut(&mut self) -> &mut BaseDocument {
        self.doc
    }
}

impl LayoutPassState<'_> {
    pub(super) fn node_from_id(&self, node_id: taffy::prelude::NodeId) -> &Node {
        &self.nodes[dom_node_id(node_id)]
    }

    pub(super) fn node_from_id_mut(&mut self, node_id: taffy::prelude::NodeId) -> &mut Node {
        &mut self.nodes[dom_node_id(node_id)]
    }
}

impl LayoutPassState<'_> {
    /// Run the node's layout algorithm, then lay out the out-of-flow (absolute/fixed)
    /// boxes for which it is the containing block. Must be called inside the layout
    /// cache wrapper so that cache hits do not re-run the out-of-flow pass.
    pub(super) fn compute_child_layout_internal(
        &mut self,
        node_id: NodeId,
        inputs: taffy::tree::LayoutInput,
        block_ctx: Option<&mut BlockContext<'_>>,
    ) -> taffy::tree::LayoutOutput {
        #[cfg(feature = "writing-mode")]
        {
            self.compute_child_layout_in_own_wm(node_id, inputs, block_ctx)
        }
        #[cfg(not(feature = "writing-mode"))]
        {
            self.compute_child_layout_in_current_wm(node_id, inputs, block_ctx)
        }
    }

    /// Lay out `node_id` in the writing mode of the currently running algorithm (its parent's)
    pub(crate) fn compute_child_layout_in_current_wm(
        &mut self,
        node_id: NodeId,
        inputs: taffy::tree::LayoutInput,
        block_ctx: Option<&mut BlockContext<'_>>,
    ) -> taffy::tree::LayoutOutput {
        let mut output = self.dispatch_child_layout(node_id, inputs, block_ctx);
        if inputs.run_mode == RunMode::PerformLayout {
            compute_oof_layout(self, node_id, &mut output);
        }
        output
    }

    /// The style of a node being laid out *by* the node whose algorithm is currently running,
    /// expressed in that node's writing mode (see `layout::writing_mode`).
    #[inline]
    pub(crate) fn child_layout_style<'a>(
        &self,
        node: &'a Node,
    ) -> stylo_taffy::TaffyStyloStyle<ComputedStyleRef<'a>> {
        #[cfg(feature = "writing-mode")]
        {
            let mut style = node.layout_style_in(self.layout_wm);
            style.align_axis_is_inline = self.current_align_axis_is_inline;
            style
        }
        #[cfg(not(feature = "writing-mode"))]
        {
            node.layout_style()
        }
    }
}
