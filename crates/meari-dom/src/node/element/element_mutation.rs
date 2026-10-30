use super::*;

impl ElementData {
    // Copy-on-write: once Stylo has put this block in the rule tree it may be
    // shared with other elements' styles (mirrors Gecko's
    // nsDOMCSSDeclaration::EnsureBlockMutable).
    pub(super) fn mutable_style_attribute(
        &mut self,
        guard: &SharedRwLock,
    ) -> &ServoArc<Locked<PropertyDeclarationBlock>> {
        let cloned = match &self.style_attribute {
            None => Some(PropertyDeclarationBlock::new()),
            Some(arc) => {
                let read = guard.read();
                let block = arc.read_with(&read);
                block
                    .immutable
                    .load(std::sync::atomic::Ordering::Relaxed)
                    .then(|| block.clone())
            }
        };
        if let Some(block) = cloned {
            self.style_attribute = Some(ServoArc::new(guard.wrap(block)));
        }
        self.style_attribute.as_ref().unwrap()
    }

    pub fn remove_style_property(
        &mut self,
        name: &str,
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

        if self.style_attribute.is_none() {
            return false;
        }
        let style = self.mutable_style_attribute(guard);
        let mut guard = guard.write();
        let style = style.write_with(&mut guard);
        if let Some(index) = style.first_declaration_to_remove(&property_id) {
            style.remove_property(&property_id, index);
            return true;
        }

        false
    }

    pub fn set_sub_document(&mut self, sub_document: Box<dyn Document>) {
        self.special_data = SpecialElementData::SubDocument(sub_document);
    }

    pub fn remove_sub_document(&mut self) {
        self.special_data = SpecialElementData::None;
    }

    #[cfg(feature = "custom-widget")]
    pub fn set_custom_widget(&mut self, widget: Box<dyn crate::Widget>) {
        use crate::node::custom_widget::CustomWidgetData;
        self.special_data = SpecialElementData::CustomWidget(CustomWidgetData::new(widget));
    }

    #[cfg(feature = "custom-widget")]
    pub fn remove_custom_widget(&mut self) -> Vec<anyrender::ResourceId> {
        let resource_ids = self
            .custom_widget_data_mut()
            .map(|widget_data| widget_data.take_resource_ids())
            .unwrap_or_default();
        self.special_data = SpecialElementData::None;
        resource_ids
    }

    pub fn take_inline_layout(&mut self) -> Option<Box<TextLayout>> {
        std::mem::take(&mut self.inline_layout_data)
    }

    pub(crate) fn is_button_input(&self) -> bool {
        self.name.local == local_name!("input")
            && matches!(
                self.attr(local_name!("type")),
                Some("button" | "submit" | "reset")
            )
    }

    pub fn is_submit_button(&self) -> bool {
        if self.name.local != local_name!("button") {
            return false;
        }
        let type_attr = self.attr(local_name!("type"));
        let is_submit = type_attr == Some("submit");
        let is_auto_submit = type_attr.is_none()
            && self.attr(LocalName::from("command")).is_none()
            && self.attr(LocalName::from("commandfor")).is_none();
        is_submit || is_auto_submit
    }
}

impl RasterImageData {
    pub fn new(width: u32, height: u32, data: Arc<Vec<u8>>) -> Self {
        Self {
            width,
            height,
            data: Blob::new(data),
        }
    }
}

impl ImageResourceData {
    pub fn new(url: ServoArc<Url>) -> Self {
        Self {
            url,
            status: Status::Loading,
            image: ImageData::None,
        }
    }
}

impl std::fmt::Debug for SpecialElementData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpecialElementData::SubDocument(_) => f.write_str("NodeSpecificData::SubDocument"),
            #[cfg(feature = "custom-widget")]
            SpecialElementData::CustomWidget(_) => f.write_str("NodeSpecificData::CustomWidget"),
            SpecialElementData::Stylesheet(_) => f.write_str("NodeSpecificData::Stylesheet"),
            SpecialElementData::Image(data) => match **data {
                ImageData::Raster(_) => f.write_str("NodeSpecificData::Image(Raster)"),
                #[cfg(feature = "svg")]
                ImageData::Svg(_) => f.write_str("NodeSpecificData::Image(Svg)"),
                ImageData::None => f.write_str("NodeSpecificData::Image(None)"),
            },
            SpecialElementData::Canvas(_) => f.write_str("NodeSpecificData::Canvas"),
            SpecialElementData::TableRoot(_) => f.write_str("NodeSpecificData::TableRoot"),
            SpecialElementData::TextInput(_) => f.write_str("NodeSpecificData::TextInput"),
            SpecialElementData::CheckboxInput(_) => f.write_str("NodeSpecificData::CheckboxInput"),
            #[cfg(feature = "file-input")]
            SpecialElementData::FileInput(_) => f.write_str("NodeSpecificData::FileInput"),
            SpecialElementData::None => f.write_str("NodeSpecificData::None"),
        }
    }
}

impl std::fmt::Debug for ListItemLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ListItemLayout - marker {:?}", self.marker)
    }
}
