use super::*;

impl<'doc> DocumentMutator<'doc> {
    pub(super) fn unload_stylesheet(&mut self, node_id: NodeId) {
        let node = &mut self.doc.nodes[node_id];
        let Some(element) = node.element_data_mut() else {
            unreachable!();
        };
        let SpecialElementData::Stylesheet(stylesheet) = element.special_data.take() else {
            unreachable!();
        };

        let guard = self.doc.guard.read();
        self.doc.stylist.remove_stylesheet(stylesheet, &guard);
        self.doc
            .stylist
            .force_stylesheet_origins_dirty(OriginSet::all());

        self.doc.nodes_to_stylesheet.remove(&node_id);
        self.doc.stylesheet_generation += 1;
    }

    pub(super) fn load_image(&mut self, target_id: NodeId) {
        let node = &self.doc.nodes[target_id];
        if let Some(raw_src) = node.attr(local_name!("src")) {
            if !raw_src.is_empty() {
                let src = self.doc.resolve_url(raw_src);
                let src_string = src.as_str();

                // Check cache first
                if let Some(cached_image) = self.doc.image_cache.get(src_string) {
                    #[cfg(feature = "tracing")]
                    tracing::info!("Loading image {src_string} from cache");
                    let node = &mut self.doc.nodes[target_id];
                    node.element_data_mut().unwrap().special_data =
                        SpecialElementData::Image(Box::new(cached_image.clone()));
                    node.clear_layout_cache();
                    node.insert_damage(ALL_DAMAGE);
                    return;
                }

                // Check if there's already a pending request for this URL
                if let Some(waiting_list) = self.doc.pending_images.get_mut(src_string) {
                    #[cfg(feature = "tracing")]
                    tracing::info!("Image {src_string} already pending, queueing node {target_id}");
                    waiting_list.push((target_id, ImageType::Image));
                    return;
                }

                // Start fetch and track as pending
                #[cfg(feature = "tracing")]
                tracing::info!("Fetching image {src_string}");
                self.doc
                    .pending_images
                    .insert(src_string.to_string(), vec![(target_id, ImageType::Image)]);

                self.doc.net_provider.fetch(
                    self.doc.id(),
                    self.doc.build_request(src),
                    ResourceHandler::boxed(
                        self.doc.tx.clone(),
                        self.doc.id(),
                        None, // Don't pass node_id, we'll handle it via pending_images
                        self.doc.shell_provider.clone(),
                        ImageHandler::new(ImageType::Image),
                    ),
                );
            }
        }
    }

    pub(super) fn load_iframe(&mut self, target_id: NodeId) {
        if self.doc.subdocument_depth >= crate::iframe::MAX_SUBDOCUMENT_DEPTH {
            #[cfg(feature = "tracing")]
            tracing::warn!(
                "Not loading iframe: max sub-document nesting depth ({}) reached",
                crate::iframe::MAX_SUBDOCUMENT_DEPTH
            );
            return;
        }

        let node = &self.doc.nodes[target_id];
        let Some(element) = node.element_data() else {
            return;
        };

        // `srcdoc` takes precedence over `src`
        if let Some(srcdoc) = element.attr(local_name!("srcdoc")) {
            let srcdoc = srcdoc.to_string();
            self.doc.load_iframe_srcdoc(target_id, &srcdoc);
            return;
        }

        let Some(raw_src) = element.attr(local_name!("src")) else {
            return;
        };
        if raw_src.is_empty() {
            return;
        }
        let Some(url) = self.doc.url.resolve_relative(raw_src) else {
            #[cfg(feature = "tracing")]
            tracing::warn!("Not loading iframe: could not resolve url {raw_src}");
            return;
        };
        self.doc.start_iframe_load(target_id, url);
    }

    pub(super) fn load_custom_paint_src(&mut self, target_id: NodeId) {
        let node = &mut self.doc.nodes[target_id];
        if let Some(raw_src) = node.attr(local_name!("src")) {
            if let Ok(custom_paint_source_id) = raw_src.parse::<u64>() {
                self.recompute_is_animating = true;
                let canvas_data = SpecialElementData::Canvas(CanvasData {
                    custom_paint_source_id,
                });
                node.element_data_mut().unwrap().special_data = canvas_data;
            }
        }
    }

    pub(super) fn process_button_input(&mut self, target_id: NodeId) {
        let node = &self.doc.nodes[target_id];
        let Some(data) = node.element_data() else {
            return;
        };

        let tagname = data.name.local.as_ref();
        let type_attr = data.attr(local_name!("type"));
        let value = data.attr(local_name!("value"));

        // Add content of "value" attribute as a text node child if:
        //   - Tag name is
        if let ("input", Some("button" | "submit" | "reset"), Some(value)) =
            (tagname, type_attr, value)
        {
            let value = value.to_string();
            let id = self.create_text_node(&value);
            self.append_children(target_id, &[id]);
            return;
        }
        #[cfg(feature = "file-input")]
        if let ("input", Some("file")) = (tagname, type_attr) {
            let button_id = self.create_element(
                qual_name!("button", html),
                vec![
                    Attribute {
                        name: qual_name!("type", html),
                        value: "button".to_string(),
                    },
                    Attribute {
                        name: qual_name!("tabindex", html),
                        value: "-1".to_string(),
                    },
                ],
            );
            let label_id = self.create_element(qual_name!("label", html), vec![]);
            let text_id = self.create_text_node("No File Selected");
            let button_text_id = self.create_text_node("Browse");
            self.append_children(target_id, &[button_id, label_id]);
            self.append_children(label_id, &[text_id]);
            self.append_children(button_id, &[button_text_id]);
        }
    }
}

/// Set 'checked' state on an input based on given attributevalue
pub(super) fn set_input_checked_state(element: &mut ElementData, value: String) {
    let Ok(checked) = value.parse() else {
        return;
    };
    match element.special_data {
        SpecialElementData::CheckboxInput(_) => element.set_checkbox_input_checked(checked),
        // If we have just constructed the element, set the node attribute,
        // and NodeSpecificData will be created from that later
        // this simulates the checked attribute being set in html,
        // and the element's checked property being set from that
        SpecialElementData::None => element.attrs.push(Attribute {
            name: qual_name!("checked", html),
            value: checked.to_string(),
        }),
        _ => {}
    }
}

impl ViewportMut<'_> {
    pub fn new(doc: &mut BaseDocument) -> ViewportMut<'_> {
        let initial_viewport = doc.viewport.clone();
        ViewportMut {
            doc,
            initial_viewport,
        }
    }
}

impl Deref for ViewportMut<'_> {
    type Target = Viewport;

    fn deref(&self) -> &Self::Target {
        &self.doc.viewport
    }
}

impl DerefMut for ViewportMut<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.doc.viewport
    }
}

impl Drop for ViewportMut<'_> {
    fn drop(&mut self) {
        if self.doc.viewport == self.initial_viewport {
            return;
        }

        let changes = DeviceChanges::from_viewports(&self.initial_viewport, &self.doc.viewport);
        self.doc.queue_device_changes(changes);
    }
}
