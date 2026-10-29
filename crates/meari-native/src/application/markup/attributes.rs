//! Split from the parent module to keep files under the 600-line structure limit.
use crate::domain::widgets::{self};
use meari::domain::{Node, NodeKind, design::components};

pub(super) fn descendant(node: &Node, suffix: &str, path: &str) -> Option<String> {
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

pub(super) fn safe_attribute(name: &str) -> bool {
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

pub(super) fn html_tag(tag: &str) -> Option<&str> {
    super::super::groups::tag(tag)
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
                | "pre" | "video" | "form" | "a" | "abbr" | "address" | "b" | "blockquote"
                | "br" | "caption" | "code" | "del" | "figcaption" | "figure" | "i" | "kbd"
                | "mark" | "progress" | "s" | "small" | "sub" | "sup" | "time" | "u" => tag,
                _ => return None,
            })
        })
}

pub(super) fn recipe(tag: &str, node: &Node) -> String {
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
    format!("{core} {extra} {}", super::super::groups::recipe(tag, node))
}

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
