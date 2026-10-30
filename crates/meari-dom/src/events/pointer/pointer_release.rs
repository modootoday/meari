use super::*;

pub(crate) fn handle_pointerup<F: FnMut(DomEvent)>(
    doc: &mut BaseDocument,
    target: NodeId,
    event: &BlitzPointerEvent,
    mut dispatch_event: F,
) {
    if doc.devtools().highlight_hover {
        let mut node = doc.get_node(target).unwrap();
        if event.button == MouseEventButton::Secondary {
            if let Some(parent_id) = node.layout_parent.get() {
                node = doc.get_node(parent_id).unwrap();
            }
        }
        doc.debug_log_node(node.id);
        doc.devtools_mut().highlight_hover = false;
        return;
    }

    // Reset Document's drag state to DragMode::None, storing the state
    // locally for use within this function
    let drag_mode = doc.drag_mode.take();

    // Don't dispatch click if we were doing a text selection drag or panning
    // the document with a touch
    let do_click = drag_mode == DragMode::None;

    // Repaint so a dragged scrollbar thumb drops its active styling, and
    // restart its fade-out delay now that the drag no longer holds it shown
    if let DragMode::ScrollbarDrag(state) = &drag_mode {
        doc.show_scrollbars(state.scrollbar.node_id);
        doc.shell_provider.request_redraw();
    }

    let time_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;

    if let DragMode::Panning(state) = &drag_mode {
        if let Some(fling) = state.generate_fling(time_ms) {
            doc.scroll_animation = ScrollAnimationState::Fling(fling);
            doc.shell_provider.request_redraw();
        }
    }

    // Dispatch a click event
    if do_click && event.button == MouseEventButton::Main {
        dispatch_event(DomEvent::new(target, DomEventData::Click(event.clone())));
    }

    // Dispatch a context menu event
    if do_click && event.button == MouseEventButton::Secondary {
        dispatch_event(DomEvent::new(
            target,
            DomEventData::ContextMenu(event.clone()),
        ));
    }
}

pub(crate) fn handle_click(
    doc: &mut BaseDocument,
    target: NodeId,
    event: &BlitzPointerEvent,
    dispatch_event: &mut dyn FnMut(DomEvent),
) {
    let double_click_event = event.clone();

    let mut maybe_node_id = Some(target);
    let matched = 'matched: {
        while let Some(node_id) = maybe_node_id {
            let maybe_element = {
                let node = &mut doc.nodes[node_id];
                node.data.downcast_element_mut()
            };

            let Some(el) = maybe_element else {
                maybe_node_id = doc.nodes[node_id].parent;
                continue;
            };

            let disabled = el.attr(local_name!("disabled")).is_some();
            if disabled {
                break 'matched true;
            }

            if let SpecialElementData::TextInput(_) = el.special_data {
                break 'matched true;
            }

            match el.name.local {
                local_name!("input") if el.attr(local_name!("type")) == Some("checkbox") => {
                    let mut is_checked = false;
                    doc.snapshot_node_and(node_id, ElementState::CHECKED, |node| {
                        if let Some(el) = node.element_data_mut() {
                            is_checked = BaseDocument::toggle_checkbox(el);
                        }
                        node.mark_ancestors_dirty();
                    });
                    let value = is_checked.to_string();
                    dispatch_event(DomEvent::new(
                        node_id,
                        DomEventData::Input(BlitzInputEvent { value }),
                    ));
                    generate_focus_events(
                        doc,
                        &mut |doc| {
                            doc.set_focus_to(node_id);
                        },
                        dispatch_event,
                    );
                    break 'matched true;
                }
                local_name!("input") if el.attr(local_name!("type")) == Some("radio") => {
                    if let Some(radio_set) = el.attr(local_name!("name")).map(str::to_string) {
                        BaseDocument::toggle_radio(doc, radio_set, node_id);
                    } else if el.checkbox_input_checked().is_some() {
                        doc.snapshot_node_and(node_id, ElementState::CHECKED, |node| {
                            if let Some(el) = node.element_data_mut() {
                                el.set_checkbox_input_checked(true);
                            }
                            node.mark_ancestors_dirty();
                        });
                    }

                    // TODO: make input event conditional on value actually changing
                    let value = String::from("true");
                    dispatch_event(DomEvent::new(
                        node_id,
                        DomEventData::Input(BlitzInputEvent { value }),
                    ));

                    generate_focus_events(
                        doc,
                        &mut |doc| {
                            doc.set_focus_to(node_id);
                        },
                        dispatch_event,
                    );

                    break 'matched true;
                }
                // Activating the first <summary> of a <details> element toggles
                // the details' `open` attribute (expand/collapse).
                local_name!("summary") => {
                    if let Some(parent_id) = doc.nodes[node_id].parent {
                        let parent = &doc.nodes[parent_id];
                        let is_first_summary = parent
                            .data
                            .is_element_with_tag_name(&local_name!("details"))
                            && parent.children.iter().copied().find(|&child_id| {
                                doc.nodes[child_id]
                                    .data
                                    .is_element_with_tag_name(&local_name!("summary"))
                            }) == Some(node_id);

                        if is_first_summary {
                            doc.toggle_details_open(parent_id);
                            generate_focus_events(
                                doc,
                                &mut |doc| {
                                    doc.set_focus_to(node_id);
                                },
                                dispatch_event,
                            );
                            break 'matched true;
                        }
                    }
                }
                // Clicking labels triggers click, and possibly input event, of associated input
                local_name!("label") => {
                    if let Some(target_node_id) =
                        doc.label_bound_input_element(node_id).map(|n| n.id)
                    {
                        // Apply default click event action for target node
                        let target_node = doc.get_node_mut(target_node_id).unwrap();
                        let syn_event = target_node.synthetic_click_event_data(event.mods);
                        handle_click(doc, target_node_id, &syn_event, dispatch_event);
                        break 'matched true;
                    }
                }
                local_name!("a") => {
                    if let Some(href) = el.attr(local_name!("href")).map(str::to_string) {
                        if let Some(url) = doc.url.resolve_relative(&href) {
                            // If the link only differs from the current document URL by its
                            // fragment (this includes links whose href is just `#fragment`),
                            // perform in-page fragment navigation (scrolling) instead of a
                            // full navigation.
                            if url.fragment().is_some() && doc.url.is_same_document(&url) {
                                doc.scroll_to_fragment(url.fragment().unwrap_or_default());
                            } else {
                                doc.navigation_provider.navigate_to(NavigationOptions::new(
                                    url,
                                    None,
                                    doc.id(),
                                ));
                            }
                        } else {
                            #[cfg(feature = "tracing")]
                            tracing::warn!("{href} is not parseable as a url. : {:?}", *doc.url);
                        }
                        break 'matched true;
                    } else {
                        #[cfg(feature = "tracing")]
                        tracing::info!("Clicked link without href: {:?}", el.attrs());
                    }
                }
                local_name!("input") | local_name!("button")
                    if el.is_submit_button() || el.attr(local_name!("type")) == Some("submit") =>
                {
                    if let Some(form_owner) = doc.controls_to_form.get(&node_id) {
                        doc.submit_form(*form_owner, node_id);
                    }
                }
                #[cfg(feature = "file-input")]
                local_name!("input") if el.attr(local_name!("type")) == Some("file") => {
                    use crate::qual_name;
                    //TODO: Handle accept attribute https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Attributes/accept by passing an appropriate filter
                    let multiple = el.attr(local_name!("multiple")).is_some();
                    let files = doc.shell_provider.open_file_dialog(multiple, None);

                    if let Some(file) = files.first() {
                        el.attrs
                            .set(qual_name!("value", html), &file.to_string_lossy());
                    }
                    let text_content = match files.len() {
                        0 => "No Files Selected".to_string(),
                        1 => files
                            .first()
                            .unwrap()
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .to_string(),
                        x => format!("{x} Files Selected"),
                    };

                    if files.is_empty() {
                        el.special_data = SpecialElementData::None;
                    } else {
                        el.special_data = SpecialElementData::FileInput(files.into())
                    }
                    let child_label_id = doc.nodes[node_id].children[1];
                    let child_text_id = doc.nodes[child_label_id].children[0];
                    let text_data = doc.nodes[child_text_id]
                        .text_data_mut()
                        .expect("Text data not found");
                    text_data.content = text_content;
                }
                _ => {}
            }

            // No match. Recurse up to parent.
            maybe_node_id = doc.nodes[node_id].parent;
        }

        // Didn't match anything
        false
    };

    // If nothing is matched then clear focus
    if !matched {
        generate_focus_events(doc, &mut |doc| doc.clear_focus(), dispatch_event);
    }

    // Dispatch double-click event if this is the second click in quick succession
    // (click_count was already computed in handle_mousedown)
    if doc.click_count == 2 {
        dispatch_event(DomEvent::new(
            target,
            DomEventData::DoubleClick(double_click_event),
        ));
    }
}

pub(crate) fn handle_wheel<F: FnMut(DomEvent)>(
    doc: &mut BaseDocument,
    _: NodeId,
    event: BlitzWheelEvent,
    mut dispatch_event: F,
) {
    let (scroll_x, scroll_y) = match event.delta {
        BlitzWheelDelta::Lines(x, y) => (x * 20.0, y * 20.0),
        BlitzWheelDelta::Pixels(x, y) => (x, y),
    };

    let has_changed = doc.scroll_chain_by(
        doc.get_hover_node_id(),
        scroll_x,
        scroll_y,
        &mut dispatch_event,
    );
    if has_changed {
        doc.shell_provider.request_redraw();
    }
}
