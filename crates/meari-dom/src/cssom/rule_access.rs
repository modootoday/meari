use super::*;

impl CssomError {
    /// The `DOMException` name corresponding to this error
    pub fn dom_exception_name(self) -> &'static str {
        match self {
            Self::NotFound => "NotFoundError",
            Self::Syntax => "SyntaxError",
            Self::IndexSize => "IndexSizeError",
            Self::HierarchyRequest => "HierarchyRequestError",
            Self::InvalidState => "InvalidStateError",
            Self::NotSupported => "NotSupportedError",
        }
    }
}

impl From<RulesMutateError> for CssomError {
    fn from(err: RulesMutateError) -> Self {
        match err {
            RulesMutateError::Syntax => Self::Syntax,
            RulesMutateError::IndexSize => Self::IndexSize,
            RulesMutateError::HierarchyRequest => Self::HierarchyRequest,
            RulesMutateError::InvalidState => Self::InvalidState,
        }
    }
}

pub(super) fn child_rule_list(rule: &CssRule, guard: &SharedRwLockReadGuard) -> Option<RuleList> {
    let rules = match rule {
        CssRule::Style(r) => r.read_with(guard).rules.clone()?,
        CssRule::Media(r) => r.rules.clone(),
        CssRule::Supports(r) => r.rules.clone(),
        CssRule::Container(r) => r.rules.clone(),
        CssRule::Page(r) => r.read_with(guard).rules.clone(),
        CssRule::Document(r) => r.rules.clone(),
        CssRule::LayerBlock(r) => r.rules.clone(),
        CssRule::Scope(r) => r.rules.clone(),
        CssRule::StartingStyle(r) => r.rules.clone(),
        CssRule::Keyframes(r) => return Some(RuleList::Keyframes(r.clone())),
        _ => return None,
    };
    Some(RuleList::Css(rules))
}

pub(super) fn declaration_target(
    handle: &RuleHandle,
    guard: &SharedRwLockReadGuard,
) -> Option<DeclarationTarget> {
    let (block, rule_type) = match handle {
        RuleHandle::Keyframe(k) => (k.read_with(guard).block.clone(), CssRuleType::Keyframe),
        RuleHandle::Rule(rule) => match rule {
            CssRule::Style(r) => (r.read_with(guard).block.clone(), CssRuleType::Style),
            CssRule::Page(r) => (r.read_with(guard).block.clone(), CssRuleType::Page),
            CssRule::Margin(r) => (r.block.clone(), CssRuleType::Margin),
            CssRule::NestedDeclarations(r) => (
                r.read_with(guard).block.clone(),
                CssRuleType::NestedDeclarations,
            ),
            CssRule::PositionTry(r) => (r.read_with(guard).block.clone(), CssRuleType::PositionTry),
            CssRule::FontFace(r) => return Some(DeclarationTarget::FontFace(r.clone())),
            _ => return None,
        },
    };
    Some(DeclarationTarget::Block { block, rule_type })
}

pub(super) fn css_rule_interface(rule: &CssRule) -> &'static str {
    match rule {
        CssRule::Style(_) => "CSSStyleRule",
        CssRule::Namespace(_) => "CSSNamespaceRule",
        CssRule::Import(_) => "CSSImportRule",
        CssRule::Media(_) => "CSSMediaRule",
        CssRule::CustomMedia(_) => "CSSCustomMediaRule",
        CssRule::Container(_) => "CSSContainerRule",
        CssRule::FontFace(_) => "CSSFontFaceRule",
        CssRule::FontFeatureValues(_) => "CSSFontFeatureValuesRule",
        CssRule::FontPaletteValues(_) => "CSSFontPaletteValuesRule",
        CssRule::CounterStyle(_) => "CSSCounterStyleRule",
        CssRule::Keyframes(_) => "CSSKeyframesRule",
        CssRule::Margin(_) => "CSSMarginRule",
        CssRule::Supports(_) => "CSSSupportsRule",
        CssRule::Page(_) => "CSSPageRule",
        CssRule::Property(_) => "CSSPropertyRule",
        CssRule::Document(_) => "CSSMozDocumentRule",
        CssRule::LayerBlock(_) => "CSSLayerBlockRule",
        CssRule::LayerStatement(_) => "CSSLayerStatementRule",
        CssRule::Scope(_) => "CSSScopeRule",
        CssRule::StartingStyle(_) => "CSSStartingStyleRule",
        CssRule::AppearanceBase(_) => "CSSAppearanceBaseRule",
        CssRule::PositionTry(_) => "CSSPositionTryRule",
        CssRule::NestedDeclarations(_) => "CSSNestedDeclarations",
        CssRule::ViewTransition(_) => "CSSViewTransitionRule",
    }
}

/// Join the CSS serializations of `items` with `", "`
pub(super) fn comma_separated<'a, T: ToCss + 'a>(items: impl IntoIterator<Item = &'a T>) -> String {
    let mut css = CssStringWriter::new();
    for (i, item) in items.into_iter().enumerate() {
        if i > 0 {
            css.push_str(", ");
        }
        let _ = item.to_css(&mut style_traits::CssWriter::new(&mut css));
    }
    css
}

pub(super) fn rule_attributes(
    rule: &CssRule,
    guard: &SharedRwLockReadGuard,
) -> Vec<(&'static str, String)> {
    match rule {
        CssRule::Style(r) => {
            vec![("selectorText", r.read_with(guard).selectors.to_css_string())]
        }
        CssRule::Namespace(r) => vec![
            ("namespaceURI", r.url.to_string()),
            (
                "prefix",
                r.prefix.as_ref().map(|p| p.to_string()).unwrap_or_default(),
            ),
        ],
        CssRule::Import(r) => {
            let r = r.read_with(guard);
            vec![
                ("href", r.url.as_str().to_string()),
                (
                    "media",
                    r.stylesheet
                        .media(guard)
                        .map(|m| m.to_css_string())
                        .unwrap_or_default(),
                ),
            ]
        }
        CssRule::Media(r) => vec![(
            "conditionText",
            r.media_queries.read_with(guard).to_css_string(),
        )],
        CssRule::Supports(r) => vec![("conditionText", r.condition.to_css_string())],
        CssRule::Container(r) => vec![("conditionText", r.conditions.to_css_string())],
        CssRule::Page(r) => vec![("selectorText", r.read_with(guard).selectors.to_css_string())],
        CssRule::Keyframes(r) => vec![("name", r.read_with(guard).name.to_css_string())],
        CssRule::LayerBlock(r) => vec![(
            "name",
            r.name
                .as_ref()
                .map(|n| n.to_css_string())
                .unwrap_or_default(),
        )],
        CssRule::LayerStatement(r) => vec![("nameList", comma_separated(r.names.iter()))],
        CssRule::Property(r) => {
            let mut attrs = vec![("name", r.name.to_css_string())];
            for (attr, id) in [
                ("syntax", style::properties::property::DescriptorId::Syntax),
                (
                    "inherits",
                    style::properties::property::DescriptorId::Inherits,
                ),
                (
                    "initialValue",
                    style::properties::property::DescriptorId::InitialValue,
                ),
            ] {
                let mut css = CssStringWriter::new();
                let _ = r.descriptors.get(id, &mut css);
                attrs.push((attr, css));
            }
            attrs
        }
        CssRule::CounterStyle(r) => vec![("name", r.read_with(guard).name().to_css_string())],
        CssRule::FontFeatureValues(r) => {
            vec![("fontFamily", comma_separated(r.family_names.iter()))]
        }
        CssRule::FontPaletteValues(r) => vec![
            ("name", r.name.to_css_string()),
            ("fontFamily", comma_separated(r.family_names.iter())),
            (
                "basePalette",
                r.base_palette
                    .as_ref()
                    .map(|b| b.to_css_string())
                    .unwrap_or_default(),
            ),
            ("overrideColors", comma_separated(r.override_colors.iter())),
        ],
        CssRule::Margin(r) => vec![("name", format!("{:?}", r.rule_type))],
        CssRule::PositionTry(r) => vec![("name", r.read_with(guard).name.to_css_string())],
        _ => Vec::new(),
    }
}

impl BaseDocument {
    /// The owner nodes (`<style>` / `<link>` elements) of the document's author
    /// stylesheets (`document.styleSheets`).
    ///
    /// Ordered by node id, which matches document order for stylesheets
    /// created while parsing (and is the order the stylist uses too, see
    /// `add_stylesheet_for_node`).
    pub fn stylesheet_owner_nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.nodes_to_stylesheet.keys().copied()
    }

    /// Whether `node_id` currently owns a stylesheet
    pub fn node_has_stylesheet(&self, node_id: NodeId) -> bool {
        self.nodes_to_stylesheet.contains_key(&node_id)
    }

    /// A counter which changes whenever the set of stylesheets (or the
    /// stylesheet object owned by some node) changes. CSSOM wrappers use it to
    /// detect that rule data cached on the script side is stale.
    pub fn stylesheet_generation(&self) -> u64 {
        self.stylesheet_generation
    }

    /// Whether the stylesheet owned by `node_id` has been disabled through the
    /// CSSOM (`CSSStyleSheet.disabled` / `HTMLStyleElement.disabled` /
    /// `HTMLLinkElement.disabled`). `false` if the node owns no stylesheet.
    pub fn stylesheet_disabled(&self, node_id: NodeId) -> bool {
        self.nodes_to_stylesheet
            .get(&node_id)
            .is_some_and(|sheet| sheet.0.disabled())
    }

    /// Set the disabled flag of the stylesheet owned by `node_id`. A disabled
    /// stylesheet stays associated with its owner node but its rules no longer
    /// apply to the document.
    ///
    /// Returns `false` if the node owns no stylesheet.
    pub fn set_stylesheet_disabled(&mut self, node_id: NodeId, disabled: bool) -> bool {
        let Some(sheet) = self.nodes_to_stylesheet.get(&node_id) else {
            return false;
        };
        if sheet.0.set_disabled(disabled) {
            self.stylist
                .force_stylesheet_origins_dirty(OriginSet::all());
        }
        true
    }

    pub(super) fn stylesheet_loader(&self) -> StylesheetLoader {
        StylesheetLoader {
            tx: self.tx.clone(),
            doc_id: self.id(),
            net_provider: self.net_provider.clone(),
            shell_provider: self.shell_provider.clone(),
            abort_signal: self.abort_signal.clone(),
            import_depth: 0,
        }
    }

    /// Resolve `path` to the rule it denotes. The rule's ancestors (outermost
    /// first) are passed to `on_ancestor` as they are traversed.
    pub(super) fn resolve_rule(
        sheet: &DocumentStyleSheet,
        guard: &SharedRwLockReadGuard,
        path: &[usize],
        on_ancestor: impl FnMut(&CssRule),
    ) -> Option<RuleHandle> {
        let (index, parent_path) = path.split_last()?;
        let list = Self::resolve_rule_list(sheet, guard, parent_path, on_ancestor)?;
        Some(match list {
            RuleList::Css(rules) => RuleHandle::Rule(rules.read_with(guard).0.get(*index)?.clone()),
            RuleList::Keyframes(keyframes) => {
                RuleHandle::Keyframe(keyframes.read_with(guard).keyframes.get(*index)?.clone())
            }
        })
    }

    /// Resolve `path` to the rule list owned by the rule it denotes (or the
    /// stylesheet's top-level rule list for an empty path). The ancestor rules
    /// of that list (outermost first, including the rule at `path` itself) are
    /// passed to `on_ancestor` as they are traversed.
    pub(super) fn resolve_rule_list(
        sheet: &DocumentStyleSheet,
        guard: &SharedRwLockReadGuard,
        path: &[usize],
        mut on_ancestor: impl FnMut(&CssRule),
    ) -> Option<RuleList> {
        let mut list = RuleList::Css(sheet.contents(guard).rules.clone());
        for &index in path {
            let RuleList::Css(rules) = &list else {
                return None;
            };
            let rules = rules.read_with(guard);
            let rule = rules.0.get(index)?;
            let child_list = child_rule_list(rule, guard)?;
            on_ancestor(rule);
            list = child_list;
        }
        Some(list)
    }

    /// [`Self::resolve_rule_list`], also collecting the ancestor rules
    pub(super) fn resolve_rule_list_with_ancestors(
        sheet: &DocumentStyleSheet,
        guard: &SharedRwLockReadGuard,
        path: &[usize],
    ) -> Option<(Vec<CssRule>, RuleList)> {
        let mut ancestors = Vec::with_capacity(path.len());
        let list =
            Self::resolve_rule_list(sheet, guard, path, |rule| ancestors.push(rule.clone()))?;
        Some((ancestors, list))
    }

    /// The number of rules in the rule list at `path` (`CSSRuleList.length`)
    pub fn stylesheet_rule_count(&self, node_id: NodeId, path: &[usize]) -> Option<usize> {
        let sheet = self.nodes_to_stylesheet.get(&node_id)?;
        let guard = self.guard.read();
        let list = Self::resolve_rule_list(sheet, &guard, path, |_| {})?;
        Some(match list {
            RuleList::Css(rules) => rules.read_with(&guard).0.len(),
            RuleList::Keyframes(keyframes) => keyframes.read_with(&guard).keyframes.len(),
        })
    }

    /// The CSSOM-visible attributes of the rule at `path`
    pub fn stylesheet_rule_info(&self, node_id: NodeId, path: &[usize]) -> Option<CssRuleInfo> {
        let sheet = self.nodes_to_stylesheet.get(&node_id)?;
        let guard = self.guard.read();
        let handle = Self::resolve_rule(sheet, &guard, path, |_| {})?;
        let mut css_text = CssStringWriter::new();
        let has_style = declaration_target(&handle, &guard).is_some();
        Some(match handle {
            RuleHandle::Rule(rule) => {
                let _ = rule.to_css(&guard, &mut css_text);
                CssRuleInfo {
                    interface: css_rule_interface(&rule),
                    rule_type: rule.rule_type() as u16,
                    css_text,
                    has_child_rules: child_rule_list(&rule, &guard).is_some(),
                    has_style,
                    attributes: rule_attributes(&rule, &guard),
                }
            }
            RuleHandle::Keyframe(keyframe) => {
                let keyframe = keyframe.read_with(&guard);
                let _ = keyframe.to_css(&guard, &mut css_text);
                CssRuleInfo {
                    interface: "CSSKeyframeRule",
                    rule_type: CssRuleType::Keyframe as u16,
                    css_text,
                    has_child_rules: false,
                    has_style,
                    attributes: vec![("keyText", keyframe.selector.to_css_string())],
                }
            }
        })
    }

    /// Insert a rule parsed from `rule` at `index` in the rule list at `path`
    /// (`CSSStyleSheet.insertRule` / `CSSGroupingRule.insertRule` /
    /// `CSSKeyframesRule.appendRule`). Returns the index of the inserted rule.
    pub fn stylesheet_insert_rule(
        &mut self,
        node_id: NodeId,
        path: &[usize],
        rule: &str,
        index: usize,
    ) -> Result<usize, CssomError> {
        let sheet = self
            .nodes_to_stylesheet
            .get(&node_id)
            .ok_or(CssomError::NotFound)?
            .clone();
        let loader = self.stylesheet_loader();
        let lock = &self.guard;

        let (ancestors, list) = {
            let guard = lock.read();
            Self::resolve_rule_list_with_ancestors(&sheet, &guard, path)
                .ok_or(CssomError::NotFound)?
        };

        let (new_rule, index) = match &list {
            RuleList::Css(rules) => {
                let new_rule = {
                    let guard = lock.read();
                    let contents = sheet.contents(&guard);
                    let containing_rule_types =
                        ancestors
                            .iter()
                            .fold(CssRuleTypes::default(), |types, rule| {
                                CssRuleTypes::from_bits(types.bits() | rule.rule_type().bit())
                            });
                    let parse_relative_rule_type = ancestors.last().map(|rule| rule.rule_type());
                    rules.read_with(&guard).parse_rule_for_insert(
                        lock,
                        rule,
                        contents,
                        index,
                        containing_rule_types,
                        parse_relative_rule_type,
                        Some(&loader),
                        AllowImportRules::Yes,
                    )?
                };
                {
                    let mut guard = lock.write();
                    rules
                        .write_with(&mut guard)
                        .0
                        .insert(index, new_rule.clone());
                }
                (new_rule, index)
            }
            RuleList::Keyframes(keyframes) => {
                let keyframe = {
                    let guard = lock.read();
                    let contents = sheet.contents(&guard);
                    Keyframe::parse(rule, contents, lock).map_err(|_| CssomError::Syntax)?
                };
                let index = {
                    let mut guard = lock.write();
                    let keyframes = &mut keyframes.write_with(&mut guard).keyframes;
                    keyframes.push(keyframe);
                    keyframes.len() - 1
                };
                (CssRule::Keyframes(keyframes.clone()), index)
            }
        };

        let guard = lock.read();
        if let CssRule::FontFace(_) = &new_rule {
            crate::net::fetch_font_face_rules(
                std::iter::once(&new_rule),
                self.tx.clone(),
                self.id(),
                Some(node_id),
                &self.net_provider,
                &self.shell_provider,
                &guard,
                self.abort_signal.as_ref(),
            );
        }
        let (change_kind, ancestors) = match &list {
            RuleList::Css(_) => (RuleChangeKind::Insertion, ancestors.as_slice()),
            // The keyframes rule itself changed; its ancestors exclude it
            RuleList::Keyframes(_) => (
                RuleChangeKind::Generic,
                &ancestors[..ancestors.len().saturating_sub(1)],
            ),
        };
        let ancestor_refs: Vec<CssRuleRef> = ancestors.iter().map(CssRuleRef::from).collect();
        self.stylist
            .rule_changed(&sheet, &new_rule, &guard, change_kind, &ancestor_refs);
        Ok(index)
    }

    /// Remove the rule at `index` from the rule list at `path`
    /// (`CSSStyleSheet.deleteRule` / `CSSGroupingRule.deleteRule` /
    /// `CSSKeyframesRule.deleteRule`).
    pub fn stylesheet_delete_rule(
        &mut self,
        node_id: NodeId,
        path: &[usize],
        index: usize,
    ) -> Result<(), CssomError> {
        let sheet = self
            .nodes_to_stylesheet
            .get(&node_id)
            .ok_or(CssomError::NotFound)?
            .clone();
        let lock = &self.guard;

        let (ancestors, list) = {
            let guard = lock.read();
            Self::resolve_rule_list_with_ancestors(&sheet, &guard, path)
                .ok_or(CssomError::NotFound)?
        };

        let (removed_rule, change_kind, ancestors) = match &list {
            RuleList::Css(rules) => {
                let mut guard = lock.write();
                let rules = rules.write_with(&mut guard);
                let removed = rules.0.get(index).cloned().ok_or(CssomError::IndexSize)?;
                rules.remove_rule(index)?;
                (removed, RuleChangeKind::Removal, ancestors.as_slice())
            }
            RuleList::Keyframes(keyframes) => {
                let mut guard = lock.write();
                let list = &mut keyframes.write_with(&mut guard).keyframes;
                if index >= list.len() {
                    return Err(CssomError::IndexSize);
                }
                list.remove(index);
                (
                    CssRule::Keyframes(keyframes.clone()),
                    RuleChangeKind::Generic,
                    &ancestors[..ancestors.len().saturating_sub(1)],
                )
            }
        };

        let guard = lock.read();
        let ancestor_refs: Vec<CssRuleRef> = ancestors.iter().map(CssRuleRef::from).collect();
        self.stylist
            .rule_changed(&sheet, &removed_rule, &guard, change_kind, &ancestor_refs);
        Ok(())
    }

    /// The declarations of the rule at `path`, for read-only access
    pub(super) fn rule_declaration_target(
        &self,
        node_id: NodeId,
        path: &[usize],
        guard: &SharedRwLockReadGuard,
    ) -> Option<DeclarationTarget> {
        let sheet = self.nodes_to_stylesheet.get(&node_id)?;
        let handle = Self::resolve_rule(sheet, guard, path, |_| {})?;
        declaration_target(&handle, guard)
    }

    /// The declarations of the rule at `path`, along with what is needed to
    /// notify the stylist after mutating them: the stylesheet, the rule to
    /// report as changed and its ancestors.
    pub(super) fn rule_declaration_target_mut(
        &self,
        node_id: NodeId,
        path: &[usize],
    ) -> Option<(DocumentStyleSheet, Vec<CssRule>, CssRule, DeclarationTarget)> {
        let sheet = self.nodes_to_stylesheet.get(&node_id)?.clone();
        let guard = self.guard.read();
        let mut ancestors = Vec::with_capacity(path.len());
        let handle = Self::resolve_rule(&sheet, &guard, path, |rule| ancestors.push(rule.clone()))?;
        let target = declaration_target(&handle, &guard)?;
        // For keyframes, the rule to report as changed is the owning
        // `@keyframes` rule
        let (changed_rule, ancestors) = match handle {
            RuleHandle::Rule(rule) => (rule, ancestors),
            RuleHandle::Keyframe(_) => {
                let mut ancestors = ancestors;
                let rule = ancestors.pop()?;
                (rule, ancestors)
            }
        };
        Some((sheet, ancestors, changed_rule, target))
    }
}
