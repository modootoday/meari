//! Component semantics -> HTML DOM. CSS utility syntax is kept intact for Tailwind/Stylo.
use crate::domain::widgets::{self, Family, Session};
use meari::domain::{Component, Node, NodeKind, Value, design::components};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
};

#[derive(Clone, Debug)]
pub struct Element {
    pub path: String,
    pub id: String,
    pub attributes: BTreeMap<String, String>,
    pub text_override: Option<String>,
}

pub struct Markup {
    pub html: String,
    pub classes: String,
    pub elements: Vec<Element>,
    panels: BTreeMap<(String, String), String>,
}

pub fn build(component: &Component, session: &Session) -> Result<Markup, String> {
    fn identifiers(
        node: &Node,
        path: String,
        ids: &mut BTreeMap<String, String>,
        seen: &mut BTreeSet<String>,
    ) -> Result<(), String> {
        if let NodeKind::Element { children, .. } = &node.kind {
            let id = widgets::string(node, "id")
                .unwrap_or_else(|| format!("bridge-{}", path.replace('/', "-")));
            if id.trim().is_empty() || !seen.insert(id.clone()) {
                return Err(format!("empty or duplicate DOM id: {id}"));
            }
            ids.insert(path.clone(), id);
            for (index, child) in children.iter().enumerate() {
                identifiers(child, format!("{path}/{index}"), ids, seen)?;
            }
        }
        Ok(())
    }
    let mut ids = BTreeMap::new();
    identifiers(&component.root, "0".into(), &mut ids, &mut BTreeSet::new())?;
    let mut markup = Markup {
        html: String::new(),
        classes: String::new(),
        elements: Vec::new(),
        panels: BTreeMap::new(),
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
    let mut class = recipe(tag, node);
    for attribute in attributes {
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
            "open",
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
                if !is_open {
                    attrs.insert("hidden".into(), String::new());
                }
                if [Family::Dialog, Family::AlertDialog].contains(&widget.family) {
                    attrs.insert("aria-modal".into(), "true".into());
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
                if !is_open {
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
    if let Some(modal) = session
        .top_overlay()
        .filter(|w| [Family::Dialog, Family::AlertDialog].contains(&w.family))
        && let Some(content) = &modal.content
    {
        let inside = path == *content || path.starts_with(&format!("{content}/"));
        let ancestor = content.starts_with(&format!("{path}/"));
        if !inside && !ancestor {
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
    out.elements.push(Element {
        path: path.clone(),
        id,
        attributes: attrs,
        text_override: text_override.clone(),
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
                format!("{path}/{index}"),
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
            "width",
            "height",
            "viewBox",
            "d",
            "fill",
            "stroke",
        ]
        .contains(&name)
}

fn html_tag(tag: &str) -> Option<&str> {
    super::groups::tag(tag)
        .or_else(|| components::intrinsic(tag))
        .or_else(|| {
            Some(match tag {
                "Checkbox"
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
                | "option" | "table" | "thead" | "tbody" | "tr" | "td" | "th" | "svg" | "path" => {
                    tag
                }
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
