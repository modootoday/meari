//! Component semantics -> HTML DOM. CSS utility syntax is kept intact for Tailwind/Stylo.
pub(crate) mod composition;
mod mount;
use crate::domain::widgets::{self, Family, Session};
use meari::domain::{Component, Node, NodeKind, Value, design::components};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

#[derive(Clone, Debug)]
pub struct Element {
    pub path: String,
    pub aliases: Vec<String>,
    pub dom_tag: String,
    pub id: String,
    pub attributes: BTreeMap<String, String>,
    pub text_override: Option<String>,
    pub text_runs: Vec<TextRun>,
    pub preserve_dirty_value: bool,
    pub controlled_checked: Option<bool>,
}
impl Element {
    pub fn owns_path(&self, path: &str) -> bool {
        self.path == path || self.aliases.iter().any(|alias| alias == path)
    }
    pub(crate) fn same_composition(&self, next: &Self) -> bool {
        self.dom_tag == next.dom_tag
            && self
                .aliases
                .iter()
                .map(|alias| alias.strip_prefix(&self.path))
                .eq(next
                    .aliases
                    .iter()
                    .map(|alias| alias.strip_prefix(&next.path)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRun {
    pub before: Option<String>,
    pub text: String,
}

pub struct Markup {
    pub html: String,
    pub classes: String,
    pub elements: Vec<Element>,
    panels: BTreeMap<(String, String), String>,
    force_mounted: BTreeSet<String>,
}

pub fn build(component: &Component, session: &Session) -> Result<Markup, String> {
    fn identifiers(
        node: &Node,
        path: String,
        identity: String,
        ids: &mut BTreeMap<String, String>,
        seen: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        let composed = composition::prepare(node, &path)?;
        let node = composed
            .as_ref()
            .map(|composed| &composed.node)
            .unwrap_or(node);
        let content_path = composed
            .as_ref()
            .map(|composed| composed.child_path.as_str())
            .unwrap_or(&path);
        if let NodeKind::Element { children, .. } = &node.kind {
            let identity = widgets::string(node, meari::application::repeat::IDENTITY_ATTRIBUTE)
                .map(|key| format!("key-{key}"))
                .unwrap_or(identity);
            let id = widgets::string(node, "id")
                .unwrap_or_else(|| format!("bridge-{}", identity.replace('/', "-")));
            if id.trim().is_empty() || !seen.insert(id.clone()) {
                return Err(format!("empty or duplicate DOM id: {id}"));
            }
            ids.insert(path.clone(), id.clone());
            if let Some(composed) = &composed {
                ids.insert(composed.child_path.clone(), id);
            }
            let mut unkeyed = 0usize;
            for (index, child) in children.iter().enumerate() {
                let keyed = widgets::string(child, meari::application::repeat::IDENTITY_ATTRIBUTE)
                    .is_some();
                let child_identity = if keyed {
                    // A keyed root replaces this provisional identity in identifiers().
                    identity.clone()
                } else {
                    let next = format!("{identity}/{unkeyed}");
                    unkeyed += 1;
                    next
                };
                identifiers(
                    child,
                    format!("{content_path}/{index}"),
                    child_identity,
                    ids,
                    seen,
                )?;
            }
        }
        Ok(())
    }
    let mut ids = BTreeMap::new();
    identifiers(
        &component.root,
        "0".into(),
        "0".into(),
        &mut ids,
        &mut BTreeSet::new(),
    )?;
    let mut markup = Markup {
        html: String::new(),
        classes: String::new(),
        elements: Vec::new(),
        panels: BTreeMap::new(),
        force_mounted: mount::collect(&component.root)?,
    };
    fn panels(
        node: &Node,
        path: String,
        item: Option<String>,
        session: &Session,
        out: &mut BTreeMap<(String, String), String>,
    ) {
        let NodeKind::Element { tag, children, .. } = &node.kind else {
            return;
        };
        let item = if tag == "AccordionItem" {
            widgets::string(node, "value")
        } else {
            item
        };
        if ["TabsContent", "AccordionContent"].contains(&tag.as_str())
            && let Some(value) = widgets::string(node, "value").or_else(|| item.clone())
            && let Some(widget) = session.owner(&path)
        {
            out.insert((widget.root.clone(), value), path.clone());
        }
        for (index, child) in children.iter().enumerate() {
            panels(child, format!("{path}/{index}"), item.clone(), session, out);
        }
    }
    panels(
        &component.root,
        "0".into(),
        None,
        session,
        &mut markup.panels,
    );
    visit(
        &component.root,
        "0".into(),
        None,
        session,
        &ids,
        &mut markup,
    )?;
    Ok(markup)
}

fn visit(
    node: &Node,
    path: String,
    item_value: Option<String>,
    session: &Session,
    ids: &BTreeMap<String, String>,
    out: &mut Markup,
) -> Result<(), String> {
    let composed = composition::prepare(node, &path)?;
    let node = composed
        .as_ref()
        .map(|composed| &composed.node)
        .unwrap_or(node);
    let content_path = composed
        .as_ref()
        .map(|composed| composed.child_path.as_str())
        .unwrap_or(&path);
    let NodeKind::Element {
        tag,
        attributes,
        children,
    } = &node.kind
    else {
        if let NodeKind::Text(Value::String(text)) = &node.kind {
            out.html.push_str(&escape(text));
            return Ok(());
        }
        return Err("native DOM requires bound string text".into());
    };
    let mut html_tag =
        html_tag(tag).ok_or_else(|| format!("native widget implementation unavailable: {tag}"))?;
    let file_input = html_tag == "input"
        && widgets::string(node, "type").is_some_and(|kind| kind.eq_ignore_ascii_case("file"));
    if file_input {
        html_tag = "button";
    }
    let id = ids[&path].clone();
    let mut attrs = BTreeMap::from([
        ("id".into(), id.clone()),
        ("data-bridge-path".into(), path.clone()),
    ]);
    let mut class = match composed.as_ref().map(|composed| &composed.child) {
        Some(child) => match &child.kind {
            NodeKind::Element { tag, .. } => recipe(tag, child),
            _ => unreachable!("composition validates its element child"),
        },
        None => recipe(tag, node),
    };
    for attribute in attributes {
        if attribute.name == "forceMount" {
            continue; // Validated and inherited by the mount adapter before rendering.
        }
        if ["asChild", "forceMount"].contains(&attribute.name.as_str()) {
            match attribute.value {
                Value::Bool(false) => continue,
                Value::Bool(true) => {
                    return Err(format!(
                        "native {tag}.{} requires its semantic adapter",
                        attribute.name
                    ));
                }
                _ => {
                    return Err(format!(
                        "native {tag}.{} requires bound bool",
                        attribute.name
                    ));
                }
            }
        }
        if ["Switch", "Checkbox"].contains(&tag.as_str()) && attribute.name == "checked" {
            continue;
        }
        if ["Dialog", "AlertDialog"].contains(&tag.as_str()) && attribute.name == "open" {
            continue; // The widget session owns this property; the root DOM is not <dialog>.
        }
        if tag == "RadioGroup" && attribute.name == "loop" {
            continue;
        }
        if tag == "Icon"
            && ["name", "size", "tone", "stroke", "label", "decorative"]
                .contains(&attribute.name.as_str())
        {
            continue;
        }
        if let Value::Prop(_) = &attribute.value {
            return Err(format!("unbound native prop: {}", attribute.name));
        }
        if attribute.name == "className" {
            let Value::String(value) = &attribute.value else {
                return Err("className requires string".into());
            };
            write!(class, " {value}").unwrap();
            continue;
        }
        if [
            "id",
            "variant",
            "size",
            "defaultValue",
            "defaultOpen",
            "orientation",
            "decorative",
            "modal",
            "collapsible",
            "defaultChecked",
        ]
        .contains(&attribute.name.as_str())
        {
            continue;
        }
        if attribute.name == "value" && components::module(tag).is_some() && tag != "Input" {
            continue;
        }
        let name = match attribute.name.as_str() {
            "htmlFor" => "for",
            "readOnly" => "readonly",
            "tabIndex" => "tabindex",
            "autoFocus" => "autofocus",
            name => name,
        };
        if name.starts_with("on") || !safe_attribute(name) {
            return Err(format!("native DOM prop not implemented: {tag}.{name}"));
        }
        match &attribute.value {
            Value::String(value) => {
                attrs.insert(name.into(), value.clone());
            }
            Value::Bool(value) if name.starts_with("aria-") || name.starts_with("data-") => {
                attrs.insert(name.into(), value.to_string());
            }
            Value::Bool(true)
                if [
                    "disabled",
                    "readonly",
                    "checked",
                    "multiple",
                    "required",
                    "autofocus",
                    "hidden",
                    "inert",
                    "open",
                ]
                .contains(&name) =>
            {
                attrs.insert(name.into(), String::new());
            }
            Value::Bool(false) => {}
            _ => return Err(format!("incorrect native DOM prop type: {tag}.{name}")),
        }
    }
    super::groups::attributes(tag, node, &mut attrs)?;
    let icon = if tag == "Icon" {
        let (spec, classes) = super::icons::from_node(node, &mut attrs)?;
        write!(class, " {classes}").unwrap();
        Some(spec)
    } else {
        None
    };
    if tag == "Input"
        && !attrs.contains_key("value")
        && let Some(value) = widgets::string(node, "defaultValue")
    {
        attrs.insert("value".into(), value);
    }
    let item_value = if tag == "AccordionItem" {
        widgets::string(node, "value")
    } else {
        item_value
    };
    let mut text_override = None;
    if html_tag == "textarea" {
        text_override = attrs.get("value").cloned();
    }
    if super::toggles::attributes(tag, node, &mut attrs)?.is_some() {
        class.push_str(" inline-flex items-center h-5 border border-input bg-background text-foreground data-[state=checked]:bg-primary data-[state=checked]:text-primary-foreground disabled:opacity-50");
        class.push_str(if tag == "Switch" {
            " w-9 rounded-full justify-start data-[state=checked]:justify-end"
        } else {
            " w-5 rounded justify-center [&[data-state=unchecked]>svg]:invisible"
        });
        class.push_str(" [&>svg]:shrink-0 [&>svg]:pointer-events-none");
    }
    if file_input {
        if attrs.contains_key("value") || widgets::string(node, "defaultValue").is_some() {
            return Err("file input value cannot be assigned by source".into());
        }
        attrs.insert("type".into(), "button".into());
        attrs.insert("data-file-input".into(), "true".into());
        text_override = Some("Choose files — No file chosen".into());
    }
    if let Some(widget) = session.owner(&path) {
        let content_id = widget
            .content
            .as_ref()
            .and_then(|p| ids.get(p))
            .cloned()
            .unwrap_or_default();
        let is_open = widget.open;
        match tag.as_str() {
            "RadioGroup" => {
                attrs.insert("role".into(), "radiogroup".into());
                if widgets::boolean(node, "disabled") {
                    attrs.insert("aria-disabled".into(), "true".into());
                }
                if widgets::boolean(node, "required") {
                    attrs.insert("aria-required".into(), "true".into());
                }
                use meari::domain::radio_group::Orientation;
                match widget.radio_orientation {
                    Orientation::Horizontal => {
                        attrs.insert("aria-orientation".into(), "horizontal".into());
                    }
                    Orientation::Vertical => {
                        attrs.insert("aria-orientation".into(), "vertical".into());
                    }
                    Orientation::Unspecified => {}
                }
            }
            "RadioGroupItem" => {
                let item = widget
                    .items
                    .iter()
                    .find(|item| item.path == path)
                    .ok_or("radio item missing")?;
                let selected = !widget.value.is_empty() && widget.value == item.value;
                attrs.insert("type".into(), "button".into());
                attrs.insert("role".into(), "radio".into());
                attrs.insert("aria-checked".into(), selected.to_string());
                attrs.insert(
                    "data-state".into(),
                    if selected { "checked" } else { "unchecked" }.into(),
                );
                attrs.insert("data-meari-radio-value".into(), item.value.clone());
                attrs.insert(
                    "tabindex".into(),
                    if !item.disabled && widget.active.as_deref() == Some(path.as_str()) {
                        "0"
                    } else {
                        "-1"
                    }
                    .into(),
                );
                if item.disabled {
                    attrs.insert("disabled".into(), String::new());
                    attrs.insert("aria-disabled".into(), "true".into());
                }
            }
            "TabsList" => {
                attrs.insert("role".into(), "tablist".into());
                attrs.insert(
                    "aria-orientation".into(),
                    if widget.vertical {
                        "vertical"
                    } else {
                        "horizontal"
                    }
                    .into(),
                );
            }
            "TabsTrigger" | "AccordionTrigger" => {
                let item = widget
                    .items
                    .iter()
                    .find(|i| i.path == path)
                    .ok_or("missing widget item")?;
                let selected = widget.value == item.value;
                if item.disabled {
                    attrs.insert("disabled".into(), String::new());
                    attrs.insert("aria-disabled".into(), "true".into());
                }
                if let Some(panel) = out.panels.get(&(widget.root.clone(), item.value.clone())) {
                    attrs.insert("aria-controls".into(), ids[panel].clone());
                }
                attrs.insert(
                    "data-state".into(),
                    if selected { "active" } else { "inactive" }.into(),
                );
                if tag == "TabsTrigger" {
                    attrs.insert("role".into(), "tab".into());
                    attrs.insert("aria-selected".into(), selected.to_string());
                    attrs.insert(
                        "tabindex".into(),
                        if widget.active.as_deref() == Some(&path) && !item.disabled {
                            "0"
                        } else {
                            "-1"
                        }
                        .into(),
                    );
                } else {
                    attrs.insert("aria-expanded".into(), selected.to_string());
                }
            }
            "TabsContent" | "AccordionContent" => {
                let value = widgets::string(node, "value")
                    .or_else(|| item_value.clone())
                    .ok_or("panel requires value")?;
                let selected = widget.value == value;
                attrs.insert(
                    "role".into(),
                    if tag == "TabsContent" {
                        "tabpanel"
                    } else {
                        "region"
                    }
                    .into(),
                );
                attrs.insert(
                    "data-state".into(),
                    if selected { "active" } else { "inactive" }.into(),
                );
                if !selected {
                    attrs.insert("hidden".into(), String::new());
                }
                if let Some(item) = widget.items.iter().find(|i| i.value == value) {
                    attrs.insert("aria-labelledby".into(), ids[&item.path].clone());
                }
                attrs.insert("tabindex".into(), "0".into());
            }
            "DialogTrigger"
            | "AlertDialogTrigger"
            | "PopoverTrigger"
            | "SelectTrigger"
            | "DropdownMenuTrigger" => {
                attrs.insert("aria-expanded".into(), is_open.to_string());
                attrs.insert("aria-controls".into(), content_id.clone());
                attrs.insert(
                    "aria-haspopup".into(),
                    match widget.family {
                        Family::Select => "listbox",
                        Family::Menu => "menu",
                        _ => "dialog",
                    }
                    .into(),
                );
                attrs.insert(
                    "data-state".into(),
                    if is_open { "open" } else { "closed" }.into(),
                );
                if widget.family == Family::Select {
                    attrs.insert("role".into(), "combobox".into());
                }
            }
            "DialogContent"
            | "AlertDialogContent"
            | "PopoverContent"
            | "SelectContent"
            | "DropdownMenuContent" => {
                attrs.insert(
                    "role".into(),
                    match widget.family {
                        Family::Select => "listbox",
                        Family::Menu => "menu",
                        Family::AlertDialog => "alertdialog",
                        _ => "dialog",
                    }
                    .into(),
                );
                attrs.insert(
                    "data-state".into(),
                    if is_open { "open" } else { "closed" }.into(),
                );
                attrs.insert("tabindex".into(), "-1".into());
                if !is_open && !out.force_mounted.contains(&path) {
                    attrs.insert("hidden".into(), String::new());
                }
                if [Family::Dialog, Family::AlertDialog].contains(&widget.family) {
                    if widget.modal {
                        attrs.insert("aria-modal".into(), "true".into());
                    } else {
                        attrs.remove("aria-modal");
                    }
                    if let Some(title) = descendant(node, "Title", &path) {
                        attrs.insert("aria-labelledby".into(), ids[&title].clone());
                    } else if !attrs.contains_key("aria-label") {
                        return Err("modal content requires Title or aria-label".into());
                    }
                    if let Some(description) = descendant(node, "Description", &path) {
                        attrs.insert("aria-describedby".into(), ids[&description].clone());
                    }
                }
            }
            "DialogOverlay" => {
                if !is_open && !out.force_mounted.contains(&path) {
                    attrs.insert("hidden".into(), String::new());
                }
                attrs.insert("aria-hidden".into(), "true".into());
            }
            "SelectItem" | "DropdownMenuItem" => {
                let item = widget
                    .items
                    .iter()
                    .find(|i| i.path == path)
                    .ok_or("missing widget item")?;
                attrs.insert(
                    "role".into(),
                    if tag == "SelectItem" {
                        "option"
                    } else {
                        "menuitem"
                    }
                    .into(),
                );
                attrs.insert(
                    "tabindex".into(),
                    if widget.active.as_deref() == Some(&path) && !item.disabled {
                        "0"
                    } else {
                        "-1"
                    }
                    .into(),
                );
                attrs.insert("aria-disabled".into(), item.disabled.to_string());
                if tag == "SelectItem" {
                    attrs.insert(
                        "aria-selected".into(),
                        (widget.value == item.value).to_string(),
                    );
                }
            }
            "SelectValue" => {
                text_override = Some(if widget.value.is_empty() {
                    widgets::string(node, "placeholder").unwrap_or_default()
                } else {
                    widget.value.clone()
                });
            }
            _ => {}
        }
    }
    if html_tag == "button" {
        attrs.entry("type".into()).or_insert("button".into());
    }
    if let Some(modal) = session.top_modal()
        && let Some(content) = &modal.content
    {
        let inside = path == *content || path.starts_with(&format!("{content}/"));
        let ancestor = content.starts_with(&format!("{path}/"));
        let dismiss_surface = tag == "DialogOverlay"
            && session
                .owner(&path)
                .is_some_and(|owner| owner.root == modal.root);
        if !inside && !ancestor && !dismiss_surface {
            attrs.insert("aria-hidden".into(), "true".into());
            attrs.insert("inert".into(), String::new());
        }
    }
    if tag == "Separator" || tag == "DropdownMenuSeparator" {
        attrs.insert("role".into(), "separator".into());
        attrs.insert(
            "aria-orientation".into(),
            widgets::string(node, "orientation").unwrap_or("horizontal".into()),
        );
    }
    if !class.trim().is_empty() {
        attrs.insert("class".into(), class.clone());
        write!(out.classes, " {class}").unwrap();
    }
    write!(out.html, "<{html_tag}").unwrap();
    for (name, value) in &attrs {
        write!(out.html, " {name}=\"{}\"", escape(value)).unwrap();
    }
    out.html.push('>');
    let mut text_runs = Vec::new();
    if text_override.is_none() && icon.is_none() && !matches!(tag.as_str(), "Checkbox" | "Switch") {
        let mut text = String::new();
        let mut present = false;
        for (index, child) in children.iter().enumerate() {
            match &child.kind {
                NodeKind::Text(Value::String(value)) => {
                    text.push_str(value);
                    present = true;
                }
                NodeKind::Element { .. } => {
                    if present {
                        text_runs.push(TextRun {
                            before: Some(format!("{content_path}/{index}")),
                            text: std::mem::take(&mut text),
                        });
                        present = false;
                    }
                }
                _ => return Err("unbound native text run".into()),
            }
        }
        if present {
            text_runs.push(TextRun { before: None, text });
        }
    }
    out.elements.push(Element {
        path: path.clone(),
        aliases: composed
            .as_ref()
            .map(|composed| vec![composed.child_path.clone()])
            .unwrap_or_default(),
        dom_tag: html_tag.into(),
        id,
        attributes: attrs,
        text_override: text_override.clone(),
        text_runs,
        preserve_dirty_value: matches!(html_tag, "input" | "textarea")
            && !attributes.iter().any(|attribute| attribute.name == "value"),
        controlled_checked: if html_tag == "input" {
            attributes
                .iter()
                .find(|attribute| attribute.name == "checked")
                .and_then(|attribute| match attribute.value {
                    Value::Bool(value) => Some(value),
                    _ => None,
                })
        } else {
            None
        },
    });
    if matches!(tag.as_str(), "Checkbox" | "Switch") {
        let spec = super::toggles::icon(tag);
        write!(out.classes, " {} {}", spec.size.class(), spec.tone.class()).unwrap();
        out.html.push_str(&super::icons::render(spec)?);
    } else if let Some(spec) = icon {
        out.html.push_str(&super::icons::render(spec)?);
    } else if let Some(text) = text_override {
        out.html.push_str(&escape(&text));
    } else {
        for (index, child) in children.iter().enumerate() {
            visit(
                child,
                format!("{content_path}/{index}"),
                item_value.clone(),
                session,
                ids,
                out,
            )?;
        }
    }
    if !["input", "hr", "img", "br"].contains(&html_tag) {
        write!(out.html, "</{html_tag}>").unwrap();
    } else if !children.is_empty() {
        return Err(format!("{tag} cannot have children"));
    }
    Ok(())
}

fn descendant(node: &Node, suffix: &str, path: &str) -> Option<String> {
    let NodeKind::Element { tag, children, .. } = &node.kind else {
        return None;
    };
    if tag.ends_with(suffix) {
        return Some(path.into());
    }
    children
        .iter()
        .enumerate()
        .find_map(|(i, c)| descendant(c, suffix, &format!("{path}/{i}")))
}

fn safe_attribute(name: &str) -> bool {
    name.starts_with("aria-")
        || name.starts_with("data-")
        || [
            "class",
            "style",
            "title",
            "role",
            "dir",
            "for",
            "disabled",
            "readonly",
            "checked",
            "type",
            "value",
            "placeholder",
            "name",
            "min",
            "max",
            "step",
            "multiple",
            "accept",
            "required",
            "tabindex",
            "autofocus",
            "hidden",
            "inert",
            "width",
            "height",
            "viewBox",
            "d",
            "fill",
            "stroke",
            "src",
            "alt",
        ]
        .contains(&name)
}

fn html_tag(tag: &str) -> Option<&str> {
    super::groups::tag(tag)
        .or_else(|| components::intrinsic(tag))
        .or_else(|| {
            Some(match tag {
                "Checkbox"
                | "RadioGroupItem"
                | "Switch"
                | "TabsTrigger"
                | "AccordionTrigger"
                | "DialogTrigger"
                | "DialogClose"
                | "AlertDialogTrigger"
                | "AlertDialogCancel"
                | "AlertDialogAction"
                | "PopoverTrigger"
                | "PopoverClose"
                | "SelectTrigger"
                | "DropdownMenuTrigger" => "button",
                "DialogTitle" | "AlertDialogTitle" => "h2",
                "DialogDescription" | "AlertDialogDescription" => "p",
                "SelectValue" | "Icon" => "span",
                "DropdownMenuSeparator" => "hr",
                "Tabs"
                | "RadioGroup"
                | "TabsList"
                | "TabsContent"
                | "Accordion"
                | "AccordionItem"
                | "AccordionContent"
                | "Dialog"
                | "DialogContent"
                | "DialogHeader"
                | "DialogFooter"
                | "DialogPortal"
                | "DialogOverlay"
                | "AlertDialog"
                | "AlertDialogContent"
                | "AlertDialogHeader"
                | "AlertDialogFooter"
                | "Popover"
                | "PopoverContent"
                | "Select"
                | "SelectContent"
                | "SelectItem"
                | "SelectGroup"
                | "SelectLabel"
                | "DropdownMenu"
                | "DropdownMenuContent"
                | "DropdownMenuItem"
                | "DropdownMenuLabel" => "div",
                "article" | "main" | "header" | "footer" | "nav" | "aside" | "ul" | "ol" | "li"
                | "strong" | "em" | "h1" | "h2" | "h4" | "h5" | "h6" | "textarea" | "select"
                | "option" | "table" | "thead" | "tbody" | "tr" | "td" | "th" | "svg" | "path"
                | "img" | "dl" | "dt" | "dd" | "fieldset" | "legend" | "details" | "summary"
                | "pre" | "video" => tag,
                _ => return None,
            })
        })
}

fn recipe(tag: &str, node: &Node) -> String {
    let core = components::recipe(
        tag,
        &widgets::string(node, "variant").unwrap_or("default".into()),
        &widgets::string(node, "size").unwrap_or("default".into()),
    );
    let extra = match tag {
        "TabsList" => {
            "inline-flex h-9 items-center gap-1 rounded-lg bg-muted p-1 text-muted-foreground"
        }
        "TabsTrigger" => {
            "inline-flex h-7 items-center justify-center rounded-md px-3 text-sm font-medium data-[state=active]:bg-background data-[state=active]:text-foreground focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-50"
        }
        "TabsContent" => "mt-4",
        "AccordionItem" => "border-b border-border",
        "AccordionTrigger" => "flex w-full py-4 text-sm font-medium",
        "AccordionContent" => "pb-4 text-sm",
        "DialogContent" | "AlertDialogContent" => {
            "fixed left-1/2 top-1/2 z-50 w-[400px] -translate-x-1/2 -translate-y-1/2 rounded-lg border border-border bg-background p-6 text-foreground shadow-lg"
        }
        "DialogOverlay" => "fixed inset-0 z-40 bg-black/50",
        "PopoverContent" | "SelectContent" | "DropdownMenuContent" => {
            "absolute z-50 min-w-32 rounded-md border border-border bg-popover p-2 text-popover-foreground shadow-md"
        }
        "SelectItem" | "DropdownMenuItem" => {
            "rounded-sm px-2 py-1 text-sm focus:bg-accent aria-disabled:opacity-50"
        }
        "DialogTitle" | "AlertDialogTitle" => "text-lg font-semibold",
        "DialogDescription" | "AlertDialogDescription" => "my-2 text-sm text-muted-foreground",
        "DialogTrigger"
        | "DialogClose"
        | "AlertDialogTrigger"
        | "AlertDialogCancel"
        | "AlertDialogAction"
        | "PopoverTrigger"
        | "PopoverClose"
        | "SelectTrigger"
        | "DropdownMenuTrigger" => {
            "inline-flex h-9 items-center rounded-md border border-input bg-background px-4 text-sm focus-visible:outline-2 focus-visible:outline-ring disabled:opacity-50"
        }
        _ => "",
    };
    format!("{core} {extra} {}", super::groups::recipe(tag, node))
}

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
