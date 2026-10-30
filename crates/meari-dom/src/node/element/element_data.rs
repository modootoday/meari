use super::*;

impl LayoutData {
    pub const fn new() -> Self {
        Self {
            cache: Cache::new(),
            unrounded_layout: Layout::new(),
            final_layout: Layout::new(),
            scroll_offset: crate::Point::ZERO,
            scrollable_overflow: KurboRect::ZERO,
            #[cfg(feature = "writing-mode")]
            writing_mode: stylo_taffy::WritingMode::empty(),
        }
    }
}

impl Default for LayoutData {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared read-only default returned by accessors for nodes whose
/// [`LayoutData`] has never been allocated.
pub(crate) static DEFAULT_LAYOUT_DATA: LayoutData = LayoutData::new();

impl ElementData {
    /// This element's layout output state, or a shared default if layout has
    /// never written to this element.
    #[inline]
    pub fn layout_data(&self) -> &LayoutData {
        self.layout_data.as_deref().unwrap_or(&DEFAULT_LAYOUT_DATA)
    }

    /// Mutable access to this element's layout output state, allocating it if
    /// it does not yet exist.
    #[inline]
    pub fn layout_data_mut(&mut self) -> &mut LayoutData {
        self.layout_data.get_or_insert_default()
    }
}

impl DocumentData {
    /// The document node's layout output state, or a shared default if layout
    /// has never written to it.
    #[inline]
    pub fn layout_data(&self) -> &LayoutData {
        self.layout_data.as_deref().unwrap_or(&DEFAULT_LAYOUT_DATA)
    }

    /// Mutable access to the document node's layout output state, allocating
    /// it if it does not yet exist.
    #[inline]
    pub fn layout_data_mut(&mut self) -> &mut LayoutData {
        self.layout_data.get_or_insert_default()
    }
}

// Hand-written like `ElementData`'s, because `ElementSelectorFlags` does not
// implement `Debug`. Every other field is still reported.
impl std::fmt::Debug for DocumentData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentData")
            .field("stylo_element_data", &self.stylo_element_data)
            .field("guard", &self.guard)
            .field("dirty_descendants", &self.dirty_descendants)
            .field("damaged_descendants", &self.damaged_descendants)
            .field("element_state", &self.element_state)
            .field("has_snapshot", &self.has_snapshot)
            .field("snapshot_handled", &self.snapshot_handled)
            .field("display_constructed_as", &self.display_constructed_as)
            .field("layout_data", &self.layout_data)
            .field("transform", &self.transform)
            .finish_non_exhaustive()
    }
}

impl DocumentData {
    pub fn new() -> Self {
        Self {
            stylo_element_data: Default::default(),
            selector_flags: Cell::new(ElementSelectorFlags::empty()),
            guard: None,
            dirty_descendants: AtomicBool::new(true),
            damaged_descendants: AtomicBool::new(true),
            element_state: ElementState::empty(),
            has_snapshot: false,
            snapshot_handled: AtomicBool::new(false),
            display_constructed_as: StyloDisplay::Block,
            layout_data: None,
            transform: None,
        }
    }
}

impl Default for DocumentData {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for DocumentData {
    fn clone(&self) -> Self {
        // Runtime style/layout state is reset (the document node is not
        // meaningfully cloneable), matching `ElementData`'s clone semantics.
        Self {
            guard: self.guard.clone(),
            ..Self::new()
        }
    }
}

impl std::fmt::Debug for ElementData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ElementData")
            .field("name", &self.name)
            .field("id", &self.id)
            .field("attrs", &self.attrs)
            .field("is_focussable", &self.is_focussable)
            .field("style_attribute", &self.style_attribute)
            .field("special_data", &self.special_data)
            .field("background_images", &self.background_images)
            .field("mask_images", &self.mask_images)
            .field("inline_layout_data", &self.inline_layout_data)
            .field("list_item_data", &self.list_item_data)
            .field("template_contents", &self.template_contents)
            .field("element_state", &self.element_state)
            .field("display_constructed_as", &self.display_constructed_as)
            .finish_non_exhaustive()
    }
}

impl Clone for ElementData {
    /// Clones the *content* of the element (name, attributes, style attribute,
    /// special data, etc.). Runtime style/layout state (stylo data, taffy
    /// layout, caches, pseudo-element ids, ...) is reset to its default so that
    /// the clone behaves like a freshly-created element that has not yet been
    /// styled or laid out.
    fn clone(&self) -> Self {
        Self {
            name: self.name.clone(),
            id: self.id.clone(),
            attrs: self.attrs.clone(),
            is_focussable: self.is_focussable,
            style_attribute: self.style_attribute.clone(),
            special_data: self.special_data.clone(),
            background_images: self.background_images.clone(),
            mask_images: self.mask_images.clone(),
            inline_layout_data: self.inline_layout_data.clone(),
            list_item_data: self.list_item_data.clone(),
            template_contents: self.template_contents,

            // Runtime state: reset to defaults.
            stylo_element_data: Default::default(),
            selector_flags: Cell::new(ElementSelectorFlags::empty()),
            guard: self.guard.clone(),
            element_state: self.element_state,
            has_snapshot: false,
            snapshot_handled: AtomicBool::new(false),
            dirty_descendants: AtomicBool::new(true),
            damaged_descendants: AtomicBool::new(true),
            before: None,
            after: None,
            detailed_layout_info: taffy::DetailedLayoutInfo::None,
            display_constructed_as: StyloDisplay::Block,
            layout_data: None,
            transform: None,
        }
    }
}

impl Clone for SpecialElementData {
    fn clone(&self) -> Self {
        match self {
            Self::SubDocument(_) => Self::None, // TODO
            #[cfg(feature = "custom-widget")]
            Self::CustomWidget(_) => Self::None, // TODO
            Self::Stylesheet(data) => Self::Stylesheet(data.clone()),
            Self::Image(data) => Self::Image(data.clone()),
            Self::Canvas(data) => Self::Canvas(data.clone()),
            Self::TableRoot(data) => Self::TableRoot(data.clone()),
            Self::TextInput(data) => Self::TextInput(data.clone()),
            Self::CheckboxInput(data) => Self::CheckboxInput(*data),
            #[cfg(feature = "file-input")]
            Self::FileInput(data) => Self::FileInput(data.clone()),
            Self::None => Self::None,
        }
    }
}

impl SpecialElementData {
    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }
}

impl ElementData {
    pub fn new(name: QualName, attrs: Vec<Attribute>) -> Self {
        let id_attr_atom = attrs
            .iter()
            .find(|attr| &attr.name.local == "id")
            .map(|attr| attr.value.as_ref())
            .map(|value: &str| Atom::from(value));

        let mut data = ElementData {
            name,
            id: id_attr_atom,
            attrs: Attributes::new(attrs),
            is_focussable: false,
            style_attribute: Default::default(),
            inline_layout_data: None,
            list_item_data: None,
            special_data: SpecialElementData::None,
            template_contents: None,
            background_images: ThinVec::new(),
            mask_images: ThinVec::new(),

            stylo_element_data: Default::default(),
            selector_flags: Cell::new(ElementSelectorFlags::empty()),
            guard: None,
            element_state: ElementState::empty(),
            has_snapshot: false,
            snapshot_handled: AtomicBool::new(false),
            dirty_descendants: AtomicBool::new(true),
            damaged_descendants: AtomicBool::new(true),
            before: None,
            after: None,
            detailed_layout_info: taffy::DetailedLayoutInfo::None,
            display_constructed_as: StyloDisplay::Block,
            layout_data: None,
            transform: None,
        };
        data.flush_is_focussable();
        data.flush_link_state();

        // Mirror the `checked` attribute into the element state so that `:checked`
        // selectors can be matched (and invalidated) from `ElementState`.
        if data.name.local == local_name!("input")
            && matches!(
                data.attr(local_name!("type")),
                Some("checkbox") | Some("radio")
            )
            && data.has_attr(local_name!("checked"))
        {
            data.element_state.insert(ElementState::CHECKED);
        }

        // The element state needs to be modified if the element can be disabled.
        if data.can_be_disabled() {
            data.element_state
                .insert(match data.has_attr(local_name!("disabled")) {
                    true => ElementState::DISABLED,
                    false => ElementState::ENABLED,
                });
        }

        data
    }

    pub fn attrs(&self) -> &[Attribute] {
        &self.attrs
    }

    pub fn attr(&self, name: impl PartialEq<LocalName>) -> Option<&str> {
        let attr = self.attrs.iter().find(|attr| name == attr.name.local)?;
        Some(&attr.value)
    }

    pub fn attr_parsed<T: FromStr>(&self, name: impl PartialEq<LocalName>) -> Option<T> {
        let attr = self.attrs.iter().find(|attr| name == attr.name.local)?;
        attr.value.parse::<T>().ok()
    }

    /// Detects the presence of the attribute, treating *any* value as truthy.
    pub fn has_attr(&self, name: impl PartialEq<LocalName>) -> bool {
        self.attrs.iter().any(|attr| name == attr.name.local)
    }

    pub fn can_be_disabled(&self) -> bool {
        local_names!("button", "input", "select", "textarea").contains(&self.name.local)
    }

    /// Whether this element is a link (an `<a>` or `<area>` element with an `href` attribute)
    pub fn is_link(&self) -> bool {
        (self.name.local == local_name!("a") || self.name.local == local_name!("area"))
            && self.has_attr(local_name!("href"))
    }

    /// Sync the visitedness bits of `element_state` with the element's link-ness.
    /// Blitz does not track browsing history, so all links are unvisited.
    ///
    /// Stylo's snapshot invalidation (`ElementWrapper::is_link`) determines link-ness
    /// from these state bits, so they must be kept accurate for `:link`/`:any-link`
    /// selectors to be correctly invalidated. Must be called whenever the `href`
    /// attribute is added or removed.
    pub fn flush_link_state(&mut self) {
        self.element_state
            .remove(ElementState::VISITED_OR_UNVISITED);
        if self.is_link() {
            self.element_state.insert(ElementState::UNVISITED);
        }
    }

    pub fn image_data(&self) -> Option<&ImageData> {
        match &self.special_data {
            SpecialElementData::Image(data) => Some(&**data),
            _ => None,
        }
    }

    pub fn image_data_mut(&mut self) -> Option<&mut ImageData> {
        match self.special_data {
            SpecialElementData::Image(ref mut data) => Some(&mut **data),
            _ => None,
        }
    }

    pub fn raster_image_data(&self) -> Option<&RasterImageData> {
        match self.image_data()? {
            ImageData::Raster(data) => Some(data),
            _ => None,
        }
    }

    pub fn raster_image_data_mut(&mut self) -> Option<&mut RasterImageData> {
        match self.image_data_mut()? {
            ImageData::Raster(data) => Some(data),
            _ => None,
        }
    }

    pub fn canvas_data(&self) -> Option<&CanvasData> {
        match &self.special_data {
            SpecialElementData::Canvas(data) => Some(data),
            _ => None,
        }
    }

    pub fn sub_doc_data(&self) -> Option<&dyn Document> {
        match &self.special_data {
            SpecialElementData::SubDocument(data) => Some(data.as_ref()),
            _ => None,
        }
    }

    pub fn sub_doc_data_mut(&mut self) -> Option<&mut dyn Document> {
        match &mut self.special_data {
            SpecialElementData::SubDocument(data) => Some(data.as_mut()),
            _ => None,
        }
    }

    #[cfg(feature = "svg")]
    pub fn svg_data(&self) -> Option<&usvg::Tree> {
        match self.image_data()? {
            ImageData::Svg(data) => Some(&data.tree),
            _ => None,
        }
    }

    pub fn text_input_data(&self) -> Option<&TextInputData> {
        match &self.special_data {
            SpecialElementData::TextInput(data) => Some(data),
            _ => None,
        }
    }

    pub fn text_input_data_mut(&mut self) -> Option<&mut TextInputData> {
        match &mut self.special_data {
            SpecialElementData::TextInput(data) => Some(data),
            _ => None,
        }
    }

    #[cfg(feature = "custom-widget")]
    pub fn custom_widget_data(&self) -> Option<&CustomWidgetData> {
        match &self.special_data {
            SpecialElementData::CustomWidget(data) => Some(data),
            _ => None,
        }
    }

    #[cfg(feature = "custom-widget")]
    pub fn custom_widget_data_mut(&mut self) -> Option<&mut CustomWidgetData> {
        match &mut self.special_data {
            SpecialElementData::CustomWidget(data) => Some(data),
            _ => None,
        }
    }

    pub fn checkbox_input_checked(&self) -> Option<bool> {
        match self.special_data {
            SpecialElementData::CheckboxInput(checked) => Some(checked),
            _ => None,
        }
    }

    pub fn checkbox_input_checked_mut(&mut self) -> Option<&mut bool> {
        match self.special_data {
            SpecialElementData::CheckboxInput(ref mut checked) => Some(checked),
            _ => None,
        }
    }

    /// Set the checked state of a checkbox/radio input, keeping the
    /// `ElementState::CHECKED` bit (used for `:checked` selector matching and
    /// invalidation) in sync with the special data.
    pub fn set_checkbox_input_checked(&mut self, checked: bool) {
        if let Some(is_checked) = self.checkbox_input_checked_mut() {
            *is_checked = checked;
        }
        self.element_state.set(ElementState::CHECKED, checked);
    }

    #[cfg(feature = "file-input")]
    pub fn file_data(&self) -> Option<&FileData> {
        match &self.special_data {
            SpecialElementData::FileInput(data) => Some(data),
            _ => None,
        }
    }

    #[cfg(feature = "file-input")]
    pub fn file_data_mut(&mut self) -> Option<&mut FileData> {
        match &mut self.special_data {
            SpecialElementData::FileInput(data) => Some(data),
            _ => None,
        }
    }

    pub fn flush_is_focussable(&mut self) {
        let disabled: bool = self.attr_parsed(local_name!("disabled")).unwrap_or(false);
        let tabindex: Option<i32> = self.attr_parsed(local_name!("tabindex"));
        let contains_sub_document: bool = self.sub_doc_data().is_some();

        self.is_focussable = contains_sub_document
            || (!disabled
                && match tabindex {
                    Some(index) => index >= 0,
                    None => {
                        // Some focusable HTML elements have a default tabindex value of 0 set under the hood by the user agent.
                        // These elements are:
                        //   - <a> or <area> with href attribute
                        //   - <button>, <frame>, <iframe>, <input>, <object>, <select>, <textarea>, and SVG <a> element
                        //   - <summary> element that provides summary for a <details> element.

                        if [local_name!("a"), local_name!("area")].contains(&self.name.local) {
                            self.attr(local_name!("href")).is_some()
                        } else {
                            const DEFAULT_FOCUSSABLE_ELEMENTS: [LocalName; 7] = [
                                local_name!("button"),
                                local_name!("input"),
                                local_name!("select"),
                                local_name!("textarea"),
                                local_name!("frame"),
                                local_name!("iframe"),
                                local_name!("summary"),
                            ];
                            DEFAULT_FOCUSSABLE_ELEMENTS.contains(&self.name.local)
                        }
                    }
                })
    }

    pub fn flush_style_attribute(&mut self, guard: &SharedRwLock, url_extra_data: &UrlExtraData) {
        self.style_attribute = self.attr(local_name!("style")).map(|style_str| {
            ServoArc::new(guard.wrap(parse_style_attribute(
                style_str,
                url_extra_data,
                None,
                QuirksMode::NoQuirks,
                CssRuleType::Style,
            )))
        });
    }

    pub fn set_style_property(
        &mut self,
        name: &str,
        value: &str,
        guard: &SharedRwLock,
        url_extra_data: UrlExtraData,
    ) -> bool {
        let context = ParserContext::new(
            Origin::Author,
            &url_extra_data,
            Some(CssRuleType::Style),
            ParsingMode::DEFAULT,
            QuirksMode::NoQuirks,
            /* namespaces = */ Default::default(),
            None,
            None,
            /* attr_taint = */ Default::default(),
        );

        let Ok(property_id) = PropertyId::parse(name, &context) else {
            #[cfg(feature = "tracing")]
            tracing::warn!(property = name, "Unsupported property");
            return false;
        };
        let mut source_property_declaration = SourcePropertyDeclaration::default();
        let mut parser = Parser::new(value);
        let Ok(_) = PropertyDeclaration::parse_into(
            &mut source_property_declaration,
            property_id,
            &context,
            &mut parser,
        ) else {
            #[cfg(feature = "tracing")]
            tracing::warn!(property = name, value, "Invalid property value");
            return false;
        };

        self.mutable_style_attribute(guard)
            .write_with(&mut guard.write())
            .extend(source_property_declaration.drain(), Importance::Normal);

        true
    }
}
