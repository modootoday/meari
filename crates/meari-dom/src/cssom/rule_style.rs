use super::*;

impl BaseDocument {
    /// The serialized declarations of the rule at `path` (`CSSRule.style.cssText`)
    pub fn stylesheet_rule_style_css_text(
        &self,
        node_id: NodeId,
        path: &[usize],
    ) -> Option<String> {
        let guard = self.guard.read();
        let target = self.rule_declaration_target(node_id, path, &guard)?;
        let mut css = CssStringWriter::new();
        match target {
            DeclarationTarget::Block { block, .. } => {
                let _ = block.read_with(&guard).to_css(&mut css);
            }
            DeclarationTarget::FontFace(rule) => {
                let _ = rule
                    .read_with(&guard)
                    .descriptors
                    .to_css(&mut style_traits::CssWriter::new(&mut css));
                // `Descriptors::to_css` emits a trailing space after each declaration
                let trimmed = css.trim_end().len();
                css.truncate(trimmed);
            }
        }
        Some(css)
    }

    /// The names of the declarations of the rule at `path`, in order
    /// (`CSSRule.style.length` / `.item()`)
    pub fn stylesheet_rule_style_property_names(
        &self,
        node_id: NodeId,
        path: &[usize],
    ) -> Option<Vec<String>> {
        let guard = self.guard.read();
        let target = self.rule_declaration_target(node_id, path, &guard)?;
        Some(match target {
            DeclarationTarget::Block { block, .. } => block
                .read_with(&guard)
                .declarations()
                .iter()
                .map(|decl| decl.id().name().into_owned())
                .collect(),
            DeclarationTarget::FontFace(rule) => {
                let descriptors = &rule.read_with(&guard).descriptors;
                (0..descriptors.len())
                    .filter_map(|i| descriptors.at(i))
                    .map(|id| id.name().to_string())
                    .collect()
            }
        })
    }

    /// The serialized value of `property` in the declarations of the rule at
    /// `path`, and whether it is `!important`
    /// (`CSSRule.style.getPropertyValue()` / `.getPropertyPriority()`).
    /// Returns an empty string for unset or unknown properties.
    pub fn stylesheet_rule_style_get_property(
        &self,
        node_id: NodeId,
        path: &[usize],
        property: &str,
    ) -> Option<(String, bool)> {
        let guard = self.guard.read();
        let target = self.rule_declaration_target(node_id, path, &guard)?;
        let mut css = CssStringWriter::new();
        let important = match target {
            DeclarationTarget::Block { block, .. } => {
                let Ok(property_id) = PropertyId::parse_enabled_for_all_content(property) else {
                    return Some((String::new(), false));
                };
                let block = block.read_with(&guard);
                let _ = block.property_value_to_css(&property_id, &mut css);
                block.property_priority(&property_id).important()
            }
            DeclarationTarget::FontFace(rule) => {
                let Ok(id) = FontFaceDescriptorId::from_ident(property) else {
                    return Some((String::new(), false));
                };
                let _ = rule.read_with(&guard).descriptors.get(id, &mut css);
                false
            }
        };
        Some((css, important))
    }

    /// Set `property` to `value` in the declarations of the rule at `path`
    /// (`CSSRule.style.setProperty()`). Invalid declarations are ignored, per
    /// CSSOM. An empty `value` removes the property.
    pub fn stylesheet_rule_style_set_property(
        &mut self,
        node_id: NodeId,
        path: &[usize],
        property: &str,
        value: &str,
        important: bool,
    ) -> Result<(), CssomError> {
        if value.trim().is_empty() {
            self.stylesheet_rule_style_remove_property(node_id, path, property)?;
            return Ok(());
        }
        let (sheet, ancestors, rule, target) = self
            .rule_declaration_target_mut(node_id, path)
            .ok_or(CssomError::NotFound)?;
        let url_data = self.url.url_extra_data();
        let lock = &self.guard;

        let change_kind = match target {
            DeclarationTarget::Block { block, rule_type } => {
                let Ok(property_id) = PropertyId::parse_enabled_for_all_content(property) else {
                    return Ok(());
                };
                let mut source = SourcePropertyDeclaration::default();
                if parse_one_declaration_into(
                    &mut source,
                    property_id,
                    value,
                    Origin::Author,
                    &url_data,
                    None,
                    ParsingMode::DEFAULT,
                    QuirksMode::NoQuirks,
                    rule_type,
                )
                .is_err()
                {
                    return Ok(());
                }
                let importance = if important {
                    Importance::Important
                } else {
                    Importance::Normal
                };
                let mut guard = lock.write();
                block
                    .write_with(&mut guard)
                    .extend(source.drain(), importance);
                match rule_type {
                    CssRuleType::PositionTry => RuleChangeKind::PositionTryDeclarations,
                    _ => RuleChangeKind::StyleRuleDeclarations,
                }
            }
            DeclarationTarget::FontFace(font_face) => {
                let Ok(id) = FontFaceDescriptorId::from_ident(property) else {
                    return Ok(());
                };
                let context = ParserContext::new(
                    Origin::Author,
                    &url_data,
                    Some(CssRuleType::FontFace),
                    ParsingMode::DEFAULT,
                    QuirksMode::NoQuirks,
                    Default::default(),
                    None,
                    None,
                    Default::default(),
                );
                let mut parser = Parser::new(value);
                let mut guard = lock.write();
                let descriptors = &mut font_face.write_with(&mut guard).descriptors;
                if descriptors.set(id, &context, &mut parser).is_err() {
                    return Ok(());
                }
                RuleChangeKind::Generic
            }
        };

        let guard = lock.read();
        let ancestor_refs: Vec<CssRuleRef> = ancestors.iter().map(CssRuleRef::from).collect();
        self.stylist
            .rule_changed(&sheet, &rule, &guard, change_kind, &ancestor_refs);
        Ok(())
    }

    /// Remove `property` from the declarations of the rule at `path`
    /// (`CSSRule.style.removeProperty()`). Returns the removed property's
    /// previous serialized value.
    pub fn stylesheet_rule_style_remove_property(
        &mut self,
        node_id: NodeId,
        path: &[usize],
        property: &str,
    ) -> Result<String, CssomError> {
        let (sheet, ancestors, rule, target) = self
            .rule_declaration_target_mut(node_id, path)
            .ok_or(CssomError::NotFound)?;
        let lock = &self.guard;
        let mut removed_value = CssStringWriter::new();

        let change_kind = match target {
            DeclarationTarget::Block { block, rule_type } => {
                let Ok(property_id) = PropertyId::parse_enabled_for_all_content(property) else {
                    return Ok(removed_value);
                };
                let mut guard = lock.write();
                let block = block.write_with(&mut guard);
                let _ = block.property_value_to_css(&property_id, &mut removed_value);
                let Some(first_declaration) = block.first_declaration_to_remove(&property_id)
                else {
                    return Ok(removed_value);
                };
                block.remove_property(&property_id, first_declaration);
                match rule_type {
                    CssRuleType::PositionTry => RuleChangeKind::PositionTryDeclarations,
                    _ => RuleChangeKind::StyleRuleDeclarations,
                }
            }
            DeclarationTarget::FontFace(font_face) => {
                let Ok(id) = FontFaceDescriptorId::from_ident(property) else {
                    return Ok(removed_value);
                };
                let mut guard = lock.write();
                let descriptors = &mut font_face.write_with(&mut guard).descriptors;
                let _ = descriptors.get(id, &mut removed_value);
                if !descriptors.remove(id) {
                    return Ok(removed_value);
                }
                RuleChangeKind::Generic
            }
        };

        let guard = lock.read();
        let ancestor_refs: Vec<CssRuleRef> = ancestors.iter().map(CssRuleRef::from).collect();
        self.stylist
            .rule_changed(&sheet, &rule, &guard, change_kind, &ancestor_refs);
        Ok(removed_value)
    }

    /// Replace the selectors of the style rule at `path` with those parsed
    /// from `selectors` (`CSSStyleRule.selectorText` setter). Per CSSOM, an
    /// invalid selector list leaves the rule unchanged (without throwing).
    pub fn stylesheet_rule_set_selector_text(
        &mut self,
        node_id: NodeId,
        path: &[usize],
        selectors: &str,
    ) -> Result<(), CssomError> {
        let sheet = self
            .nodes_to_stylesheet
            .get(&node_id)
            .ok_or(CssomError::NotFound)?
            .clone();
        let lock = &self.guard;

        let (ancestors, style_rule) = {
            let guard = lock.read();
            let mut ancestors = Vec::with_capacity(path.len());
            let handle =
                Self::resolve_rule(&sheet, &guard, path, |rule| ancestors.push(rule.clone()))
                    .ok_or(CssomError::NotFound)?;
            let RuleHandle::Rule(CssRule::Style(style_rule)) = handle else {
                return Err(CssomError::NotSupported);
            };
            (ancestors, style_rule)
        };

        // Nested style rules take relative selectors (`> .a`), resolved
        // against the innermost enclosing style rule or `@scope`, mirroring
        // stylo's `NestingContext`.
        let parse_relative = ancestors
            .iter()
            .rev()
            .find_map(|rule| match rule.rule_type() {
                CssRuleType::Style => Some(ParseRelative::ForNesting),
                CssRuleType::Scope => Some(ParseRelative::ForScope),
                _ => None,
            })
            .unwrap_or(ParseRelative::No);

        let new_selectors = {
            let guard = lock.read();
            let contents = sheet.contents(&guard);
            let selector_parser = SelectorParser {
                stylesheet_origin: contents.origin,
                namespaces: &contents.namespaces,
                url_data: &contents.url_data,
                for_supports_rule: false,
            };
            let mut parser = Parser::new(selectors);
            let Ok(new_selectors) =
                SelectorList::parse(&selector_parser, &mut parser, parse_relative)
            else {
                return Ok(());
            };
            new_selectors
        };

        {
            let mut guard = lock.write();
            style_rule.write_with(&mut guard).selectors = new_selectors;
        }

        let guard = lock.read();
        let rule = CssRule::Style(style_rule);
        let ancestor_refs: Vec<CssRuleRef> = ancestors.iter().map(CssRuleRef::from).collect();
        self.stylist.rule_changed(
            &sheet,
            &rule,
            &guard,
            RuleChangeKind::Generic,
            &ancestor_refs,
        );
        Ok(())
    }

    /// Replace the declarations of the rule at `path` with those parsed from
    /// `css` (`CSSRule.style.cssText` setter).
    pub fn stylesheet_rule_style_set_css_text(
        &mut self,
        node_id: NodeId,
        path: &[usize],
        css: &str,
    ) -> Result<(), CssomError> {
        let (sheet, ancestors, rule, target) = self
            .rule_declaration_target_mut(node_id, path)
            .ok_or(CssomError::NotFound)?;
        let DeclarationTarget::Block { block, rule_type } = target else {
            return Err(CssomError::NotSupported);
        };
        let new_block = style::properties::declaration_block::parse_style_attribute(
            css,
            &self.url.url_extra_data(),
            None,
            QuirksMode::NoQuirks,
            rule_type,
        );
        let lock = &self.guard;
        {
            let mut guard = lock.write();
            *block.write_with(&mut guard) = new_block;
        }
        let guard = lock.read();
        let ancestor_refs: Vec<CssRuleRef> = ancestors.iter().map(CssRuleRef::from).collect();
        let change_kind = match rule_type {
            CssRuleType::PositionTry => RuleChangeKind::PositionTryDeclarations,
            _ => RuleChangeKind::StyleRuleDeclarations,
        };
        self.stylist
            .rule_changed(&sheet, &rule, &guard, change_kind, &ancestor_refs);
        Ok(())
    }
}
