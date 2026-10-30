use super::*;

impl UserAgentStylesheet {
    pub fn parse(css: &str) -> Self {
        let lock = SharedRwLock::read_only();
        let data = Stylesheet::from_str(
            css,
            DocumentUrl::default().url_extra_data(),
            Origin::UserAgent,
            ServoArc::new(lock.wrap(MediaList::empty())),
            lock,
            None,
            None,
            QuirksMode::NoQuirks,
            AllowImportRules::No,
        );
        Self(DocumentStyleSheet(ServoArc::new(data)))
    }

    /// The default user-agent stylesheet ([`DEFAULT_CSS`]), parsed once.
    pub fn default_sheet() -> Self {
        static SHEET: LazyLock<UserAgentStylesheet> =
            LazyLock::new(|| UserAgentStylesheet::parse(DEFAULT_CSS));
        SHEET.clone()
    }

    pub fn sheet(&self) -> &DocumentStyleSheet {
        &self.0
    }
}

impl Deref for DocGuard<'_> {
    type Target = BaseDocument;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Ref(base_document) => base_document,
            Self::RefCell(refcell_guard) => refcell_guard,
            Self::RwLock(rw_lock_read_guard) => rw_lock_read_guard,
            Self::Mutex(mutex_guard) => mutex_guard,
        }
    }
}

impl Deref for DocGuardMut<'_> {
    type Target = BaseDocument;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        match self {
            Self::Ref(base_document) => base_document,
            Self::RefCell(refcell_guard) => refcell_guard,
            Self::RwLock(rw_lock_read_guard) => rw_lock_read_guard,
            Self::Mutex(mutex_guard) => mutex_guard,
        }
    }
}

impl DerefMut for DocGuardMut<'_> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            Self::Ref(base_document) => base_document,
            Self::RefCell(refcell_guard) => &mut *refcell_guard,
            Self::RwLock(rw_lock_read_guard) => &mut *rw_lock_read_guard,
            Self::Mutex(mutex_guard) => &mut *mutex_guard,
        }
    }
}

impl Document for PlainDocument {
    fn inner(&self) -> DocGuard<'_> {
        DocGuard::Ref(&self.0)
    }
    fn inner_mut(&mut self) -> DocGuardMut<'_> {
        DocGuardMut::Ref(&mut self.0)
    }
}

impl Document for BaseDocument {
    fn inner(&self) -> DocGuard<'_> {
        DocGuard::Ref(self)
    }
    fn inner_mut(&mut self) -> DocGuardMut<'_> {
        DocGuardMut::Ref(self)
    }
}

impl Document for Rc<RefCell<BaseDocument>> {
    fn inner(&self) -> DocGuard<'_> {
        DocGuard::RefCell(self.borrow())
    }

    fn inner_mut(&mut self) -> DocGuardMut<'_> {
        DocGuardMut::RefCell(self.borrow_mut())
    }
}

impl BaseDocument {
    /// Create a new (empty) [`BaseDocument`] with the specified configuration
    pub fn new(config: DocumentConfig) -> Self {
        static ID_GENERATOR: AtomicUsize = AtomicUsize::new(1);

        let id = ID_GENERATOR.fetch_add(1, Ordering::SeqCst);

        let font_ctx = config
            .font_ctx
            .map(|mut font_ctx| {
                font_ctx.source_cache.make_shared();
                // font_ctx.collection.make_shared();
                font_ctx
            })
            .unwrap_or_else(|| {
                use parley::fontique::{Collection, CollectionOptions, SourceCache};
                let mut font_ctx = FontContext {
                    source_cache: SourceCache::new_shared(),
                    collection: Collection::new(CollectionOptions {
                        shared: false,
                        system_fonts: cfg!(all(
                            feature = "system-fonts",
                            not(target_arch = "wasm32")
                        )),
                    }),
                };
                font_ctx
                    .collection
                    .register_fonts(Blob::new(Arc::new(crate::BULLET_FONT) as _), None);
                font_ctx
            });
        let font_ctx = Arc::new(Mutex::new(font_ctx));

        // Make sure we turn on stylo features *before* creating the Stylist
        style_config::set_pref!("layout.grid.enabled", true);
        style_config::set_pref!("layout.flexbox.balance", true);
        style_config::set_pref!("layout.unimplemented", true);
        style_config::set_pref!("layout.columns.enabled", true);
        style_config::set_pref!("layout.css.basic-shape-shape.enabled", true);
        style_config::set_pref!("layout.css.tree-counting-functions.enabled", true);
        style_config::set_pref!("layout.css.progress-function.enabled", true);
        style_config::set_pref!("layout.variable_fonts.enabled", true);
        #[cfg(feature = "writing-mode")]
        style_config::set_pref!("layout.writing-mode.enabled", true);
        style_config::set_pref!("layout.threads", -1);

        let viewport = config.viewport.unwrap_or_default();
        let media_type = config.media_type.unwrap_or_else(MediaType::screen);
        let device = make_device(&viewport, media_type.clone(), font_ctx.clone());
        let stylist = Stylist::new(device, QuirksMode::NoQuirks);
        let snapshots = SnapshotMap::new();
        let nodes = Box::new(NodeTree::new());
        let guard = SharedRwLock::new();
        let nodes_to_id = HashMap::new();

        let base_url = config
            .base_url
            .and_then(|url| DocumentUrl::from_str(&url).ok())
            .unwrap_or_default();

        let net_provider = config
            .net_provider
            .unwrap_or_else(|| Arc::new(DummyNetProvider));
        let navigation_provider = config
            .navigation_provider
            .unwrap_or_else(|| Arc::new(DummyNavigationProvider));
        let shell_provider = config
            .shell_provider
            .unwrap_or_else(|| Arc::new(DummyShellProvider));
        let html_parser_provider = config
            .html_parser_provider
            .unwrap_or_else(|| Arc::new(DummyHtmlParserProvider));

        let (tx, rx) = channel();

        let mut doc = Self {
            id,
            tx,
            rx: Some(rx),

            guard,
            nodes,
            root_node_id: NodeId::default(),
            stylist,
            animations: DocumentAnimationSet::default(),
            snapshots,
            nodes_to_id,
            viewport,
            media_type,
            pending_device_changes: DeviceChanges::empty(),
            style_threading: config.style_threading,
            incremental_layout: config.incremental.unwrap_or(true),
            subdocument_depth: config.subdocument_depth,
            devtool_settings: DevtoolSettings::default(),
            url: base_url,
            nodes_to_stylesheet: BTreeMap::new(),
            stylesheet_generation: 0,
            font_ctx,
            #[cfg(feature = "parallel-construct")]
            thread_font_contexts: ThreadLocal::new(),
            layout_ctx: parley::LayoutContext::new(),

            hover_node_id: None,
            hover_hit_node_id: None,
            hover_node_is_text: false,
            last_client_pointer_position: None,
            focus_node_id: None,
            active_node_id: None,
            mousedown_node_id: None,
            has_active_animations: false,
            subdoc_is_animating: false,
            has_canvas: false,
            sub_document_nodes: HashSet::new(),
            iframe_loads: HashMap::new(),

            #[cfg(feature = "custom-widget")]
            custom_widget_nodes: HashSet::new(),
            #[cfg(feature = "custom-widget")]
            pending_resource_deallocations: Vec::new(),

            changed_nodes: HashSet::new(),
            deferred_construction_nodes: Vec::new(),
            image_cache: HashMap::new(),
            pending_images: HashMap::new(),
            pending_style_image_nodes: Vec::new(),
            pending_critical_resources: HashSet::new(),
            controls_to_form: HashMap::new(),
            net_provider,
            navigation_provider,
            shell_provider,
            html_parser_provider,
            abort_signal: config.abort_signal,
            last_mousedown_time: None,
            mousedown_position: taffy::Point::ZERO,
            click_count: 0,
            drag_mode: DragMode::None,
            hovered_scrollbar: None,
            scrollbar_activity: HashMap::new(),
            scroll_animation: ScrollAnimationState::None,
            text_selection: TextSelection::default(),
        };

        // Initialise document with root Document node
        doc.root_node_id = doc.create_node(NodeData::Document(Box::default()));
        doc.root_node_mut().flags.insert(NodeFlags::IS_IN_DOCUMENT);

        for sheet in config.ua_stylesheets {
            doc.add_user_agent_stylesheet(sheet);
        }

        // Stylo data on the root node container is needed to render the node
        let stylo_element_data = StyloElementData {
            styles: ElementStyles {
                primary: Some(
                    ComputedValues::initial_values_with_font_override(Font::initial_values())
                        .to_arc(),
                ),
                ..Default::default()
            },
            ..Default::default()
        };
        let stylo_data = doc.root_node_mut().stylo_element_data_mut();
        *stylo_data.ensure_init_mut() = stylo_element_data;

        doc
    }

    /// Set the Document's networking provider
    pub fn set_net_provider(&mut self, net_provider: Arc<dyn NetProvider>) {
        self.net_provider = net_provider;
    }

    /// Set the Document's navigation provider
    pub fn set_navigation_provider(&mut self, navigation_provider: Arc<dyn NavigationProvider>) {
        self.navigation_provider = navigation_provider;
    }

    /// Set the Document's shell provider
    pub fn set_shell_provider(&mut self, shell_provider: Arc<dyn ShellProvider>) {
        self.shell_provider = shell_provider;
    }

    /// Set the Document's html parser provider
    pub fn set_html_parser_provider(&mut self, html_parser_provider: Arc<dyn HtmlParserProvider>) {
        self.html_parser_provider = html_parser_provider;
    }

    /// Set base url for resolving linked resources (stylesheets, images, fonts, etc)
    pub fn set_base_url(&mut self, url: &str) {
        self.url = DocumentUrl::from(Url::parse(url).unwrap());
    }

    /// The base url used for resolving linked resources (stylesheets, images, fonts, etc)
    pub fn base_url(&self) -> &Url {
        &self.url
    }

    pub fn guard(&self) -> &SharedRwLock {
        &self.guard
    }

    pub fn tree(&self) -> &NodeTree {
        &self.nodes
    }

    pub fn id(&self) -> usize {
        self.id
    }

    /// Wrapper around [`crate::net::stamped_request`]. Use the free function
    /// when `&self` would conflict with a held `&mut` borrow on a field.
    pub(crate) fn build_request(&self, url: url::Url) -> Request {
        crate::net::stamped_request(url, self.abort_signal.as_ref())
    }

    pub fn favicon_url(&self) -> Option<String> {
        self.tree().iter().find_map(|(_, node)| {
            let data = &node.data;
            if !data.is_element_with_tag_name(&local_name!("link")) {
                return None;
            }
            let rel = data.attr(local_name!("rel"))?;
            if !rel
                .split_ascii_whitespace()
                .any(|v| v.eq_ignore_ascii_case("icon"))
            {
                return None;
            }
            data.attr(local_name!("href")).map(|s| s.to_string())
        })
    }

    pub fn get_node(&self, node_id: NodeId) -> Option<&Node> {
        self.nodes.get(node_id)
    }

    pub fn get_node_mut(&mut self, node_id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(node_id)
    }

    pub fn get_focussed_node_id(&self) -> Option<NodeId> {
        self.focus_node_id
            .or(self.try_root_element().map(|el| el.id))
    }

    pub fn mutate<'doc>(&'doc mut self) -> DocumentMutator<'doc> {
        DocumentMutator::new(self)
    }

    pub fn handle_dom_event<F: FnMut(DomEvent)>(
        &mut self,
        event: &mut DomEvent,
        dispatch_event: F,
    ) {
        handle_dom_event(self, event, dispatch_event)
    }

    pub fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    /// Find the label's bound input elements:
    /// the element id referenced by the "for" attribute of a given label element
    /// or the first input element which is nested in the label
    /// Note that although there should only be one bound element,
    /// we return all possibilities instead of just the first
    /// in order to allow the caller to decide which one is correct
    pub fn label_bound_input_element(&self, label_node_id: NodeId) -> Option<&Node> {
        let label_element = self.nodes[label_node_id].element_data()?;
        if let Some(target_element_dom_id) = label_element.attr(local_name!("for")) {
            TreeTraverser::new(self)
                .filter_map(|id| {
                    let node = self.get_node(id)?;
                    let element_data = node.element_data()?;
                    if element_data.name.local != local_name!("input") {
                        return None;
                    }
                    let id = element_data.id.as_ref()?;
                    if *id == *target_element_dom_id {
                        Some(node)
                    } else {
                        None
                    }
                })
                .next()
        } else {
            TreeTraverser::new_with_root(self, label_node_id)
                .filter_map(|child_id| {
                    let node = self.get_node(child_id)?;
                    let element_data = node.element_data()?;
                    if element_data.name.local == local_name!("input") {
                        Some(node)
                    } else {
                        None
                    }
                })
                .next()
        }
    }

    pub fn toggle_checkbox(el: &mut ElementData) -> bool {
        let Some(is_checked) = el.checkbox_input_checked() else {
            return false;
        };
        let checked = !is_checked;
        el.set_checkbox_input_checked(checked);

        checked
    }

    pub fn toggle_radio(&mut self, radio_set_name: String, target_radio_id: NodeId) {
        let radio_ids: Vec<NodeId> = self
            .nodes
            .iter()
            .filter_map(|(i, node)| {
                let el = node.data.downcast_element()?;
                (el.attr(local_name!("name")) == Some(&radio_set_name)
                    && el.checkbox_input_checked().is_some())
                .then_some(i)
            })
            .collect();

        for i in radio_ids {
            let checked = i == target_radio_id;
            self.snapshot_node_and(i, ElementState::CHECKED, |node| {
                if let Some(el) = node.element_data_mut() {
                    el.set_checkbox_input_checked(checked);
                }
                node.mark_ancestors_dirty();
            });
        }
    }

    /// Toggle the `open` attribute of a `<details>` element, expanding or
    /// collapsing it. This is the default action triggered when the element's
    /// first `<summary>` child is activated.
    pub fn toggle_details_open(&mut self, details_id: NodeId) {
        use crate::qual_name;

        let node = &self.nodes[details_id];
        if !node.data.is_element_with_tag_name(&local_name!("details")) {
            return;
        }
        let is_open = node.data.has_attr(local_name!("open"));

        // Note: HTML attributes are in the empty (null) namespace, so the
        // QualName must not use the html namespace here, else it won't match
        // an `open` attribute created by the HTML parser.
        let mut mutator = self.mutate();
        if is_open {
            mutator.clear_attribute(details_id, qual_name!("open"));
        } else {
            mutator.set_attribute(details_id, qual_name!("open"), "");
        }
        drop(mutator);

        self.shell_provider.request_redraw();
    }

    pub fn set_style_property(&mut self, node_id: NodeId, name: &str, value: &str) {
        let node = &mut self.nodes[node_id];
        let did_change = node.element_data_mut().unwrap().set_style_property(
            name,
            value,
            &self.guard,
            self.url.url_extra_data(),
        );
        if did_change {
            node.set_restyle_hint(RestyleHint::RESTYLE_STYLE_ATTRIBUTE);
        }
    }

    pub fn remove_style_property(&mut self, node_id: NodeId, name: &str) {
        let node = &mut self.nodes[node_id];
        let did_change = node.element_data_mut().unwrap().remove_style_property(
            name,
            &self.guard,
            self.url.url_extra_data(),
        );
        if did_change {
            node.set_restyle_hint(RestyleHint::RESTYLE_STYLE_ATTRIBUTE);
        }
    }

    pub fn sub_document_node_ids(&self) -> Vec<NodeId> {
        self.sub_document_nodes.iter().copied().collect()
    }

    pub fn set_sub_document(&mut self, node_id: NodeId, sub_document: Box<dyn Document>) {
        self.nodes[node_id]
            .element_data_mut()
            .unwrap()
            .set_sub_document(sub_document);
        self.sub_document_nodes.insert(node_id);
    }

    pub fn remove_sub_document(&mut self, node_id: NodeId) {
        self.nodes[node_id]
            .element_data_mut()
            .unwrap()
            .remove_sub_document();
        self.sub_document_nodes.remove(&node_id);
        if let Some(load) = self.iframe_loads.remove(&node_id) {
            load.abort_controller.abort();
        }
    }

    /// Poll all sub-documents (see [`Document::poll`]), allowing them to make progress
    /// on any pending async operations (e.g. JavaScript timers). Hosts which poll a
    /// wrapper around a [`BaseDocument`] should call this from their `poll` implementation.
    ///
    /// Returns `true` if any sub-document reported changes.
    pub fn poll_subdocuments(&mut self, waker: Option<&Waker>) -> bool {
        let mut has_changes = false;
        for node_id in self.sub_document_nodes.iter().copied() {
            let Some(sub_doc) = self
                .nodes
                .get_mut(node_id)
                .and_then(|node| node.subdoc_mut())
            else {
                continue;
            };
            let task_context = waker.map(TaskContext::from_waker);
            has_changes |= sub_doc.poll(task_context);
        }
        has_changes
    }

    #[cfg(feature = "custom-widget")]
    pub fn custom_widget_node_ids(&self) -> Vec<NodeId> {
        self.custom_widget_nodes.iter().copied().collect()
    }

    #[cfg(feature = "custom-widget")]
    pub fn take_pending_resource_deallocations(&mut self) -> Vec<anyrender::ResourceId> {
        std::mem::take(&mut self.pending_resource_deallocations)
    }
}
