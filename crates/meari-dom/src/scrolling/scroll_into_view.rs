use super::*;

impl BaseDocument {
    /// Scroll the viewport so that the given element has the requested alignment in each axis.
    pub fn scroll_into_view(
        &mut self,
        node_id: NodeId,
        behavior: ScrollBehavior,
        vertical: ScrollLogicalPosition,
        horizontal: ScrollLogicalPosition,
    ) {
        let Some(node) = self.nodes.get(node_id) else {
            return;
        };
        let target =
            node.absolute_position(node.scroll_offset().x as f32, node.scroll_offset().y as f32);
        let target_size = node.final_layout().size;
        let Some(root_id) = self.try_root_element().map(|root| root.id) else {
            return;
        };
        let scale = self.viewport.scale() as f64;
        let viewport_width = self.viewport.window_size.0 as f64 / scale;
        let viewport_height = self.viewport.window_size.1 as f64 / scale;
        let x = Self::aligned_scroll_offset(
            self.viewport_scroll().x,
            viewport_width,
            target.x as f64,
            target_size.width as f64,
            horizontal,
        );
        let y = Self::aligned_scroll_offset(
            self.viewport_scroll().y,
            viewport_height,
            target.y as f64,
            target_size.height as f64,
            vertical,
        );
        self.scroll_to(root_id, x, y, behavior);
    }

    /// Resolve a URL fragment (the `#...` part of a URL) to a scroll target.
    ///
    /// Returns `None` if the fragment matches no element and is not a top-of-document
    /// fragment. Otherwise returns `Some(target)`, where `target` is `Some(node_id)` for
    /// the element to scroll to, or `None` to scroll to the top of the document (matching
    /// browser behaviour for empty and `top` fragments).
    pub(super) fn resolve_fragment_scroll_target(&self, fragment: &str) -> Option<Option<NodeId>> {
        // Fragments are percent-encoded in URLs (e.g. `%20`); decode before matching.
        let decoded = percent_encoding::percent_decode_str(fragment)
            .decode_utf8_lossy()
            .into_owned();

        if !decoded.is_empty() {
            if let Some(node_id) = self.get_fragment_target(&decoded) {
                return Some(Some(node_id));
            }
        }

        // An empty fragment, or the special "top" fragment when no matching element exists,
        // scrolls to the top of the document.
        if decoded.is_empty() || decoded.eq_ignore_ascii_case("top") {
            return Some(None);
        }

        None
    }

    pub(super) fn scroll_to_fragment_with_behavior(
        &mut self,
        fragment: &str,
        behavior: ScrollBehavior,
    ) -> bool {
        match self.resolve_fragment_scroll_target(fragment) {
            Some(Some(node_id)) => {
                self.scroll_into_view(
                    node_id,
                    behavior,
                    ScrollLogicalPosition::Start,
                    ScrollLogicalPosition::Nearest,
                );
                true
            }
            Some(None) => {
                let root_id = self.root_element().id;
                self.scroll_to(root_id, 0.0, 0.0, behavior);
                true
            }
            None => false,
        }
    }

    /// Scroll to the element targeted by the given URL fragment (the `#...` part of a URL).
    ///
    /// An empty fragment (or a `top` fragment that matches no element) scrolls to the top
    /// of the document, matching browser behaviour. Returns `true` if a scroll target was
    /// found.
    pub fn scroll_to_fragment(&mut self, fragment: &str) -> bool {
        self.scroll_to_fragment_with_behavior(fragment, ScrollBehavior::Auto)
    }

    /// Like [`BaseDocument::scroll_to_fragment`], but animates the viewport towards the
    /// target instead of jumping instantly. Returns `true` if a scroll target was found.
    pub fn scroll_to_fragment_smooth(&mut self, fragment: &str) -> bool {
        self.scroll_to_fragment_with_behavior(fragment, ScrollBehavior::Smooth)
    }

    pub fn resolve_scroll_animation(&mut self) {
        match &mut self.scroll_animation {
            ScrollAnimationState::Fling(fling_state) => {
                let time_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64 as f64;

                let time_diff_ms = time_ms - fling_state.last_seen_time;

                // 0.95 @ 60fps normalized to actual frame times
                let deceleration = 1.0 - ((0.05 / 16.66666) * time_diff_ms);

                fling_state.x_velocity *= deceleration;
                fling_state.y_velocity *= deceleration;
                fling_state.last_seen_time = time_ms;
                let fling_state = fling_state.clone();

                let dx = fling_state.x_velocity * time_diff_ms;
                let dy = fling_state.y_velocity * time_diff_ms;

                self.scroll_chain_by(Some(fling_state.target), dx, dy, &mut |_| {});
                if fling_state.x_velocity.abs() < 0.1 && fling_state.y_velocity.abs() < 0.1 {
                    self.scroll_animation = ScrollAnimationState::None;
                }
            }
            ScrollAnimationState::ScrollTo(scroll_to) => {
                let scroll_to = scroll_to.clone();
                let time_ms = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64 as f64;

                // Normalised progress through the animation, clamped to [0, 1].
                let progress = if scroll_to.duration <= 0.0 {
                    1.0
                } else {
                    ((time_ms - scroll_to.start_time) / scroll_to.duration).clamp(0.0, 1.0)
                };
                let eased = ease_in_out_cubic(progress);

                // Interpolate the target offset and move to it.
                let target = Point {
                    x: scroll_to.start.x + (scroll_to.end.x - scroll_to.start.x) * eased,
                    y: scroll_to.start.y + (scroll_to.end.y - scroll_to.start.y) * eased,
                };
                // TODO: dispatch `scroll` events for programmatic scrolls.
                self.write_scroll_offset(scroll_to.target, target, &mut |_| {});

                if progress >= 1.0 {
                    self.scroll_animation = ScrollAnimationState::None;
                }
            }
            ScrollAnimationState::None => {
                // Do nothing
            }
        }
    }
}
