use super::diagnostic;
use crate::domain::{Attribute, Diagnostic, Node, NodeKind, Value};

/// Local source-owned contract; it is not an upstream component implementation.
pub fn module(tag: &str) -> Option<&'static str> {
    Some(match tag {
        "Button" => "button",
        "Badge" => "badge",
        "Input" => "input",
        "Checkbox" => "checkbox",
        "Switch" => "switch",
        "Icon" => "icon",
        "Label" => "label",
        "Separator" => "separator",
        "ButtonGroup" | "ButtonGroupText" | "ButtonGroupSeparator" => "button-group",
        "Field" | "FieldSet" | "FieldLegend" | "FieldGroup" | "FieldContent" | "FieldLabel"
        | "FieldTitle" | "FieldDescription" | "FieldError" => "field",
        "Tabs" | "TabsList" | "TabsTrigger" | "TabsContent" => "tabs",
        "Accordion" | "AccordionItem" | "AccordionTrigger" | "AccordionContent" => "accordion",
        "Dialog" | "DialogTrigger" | "DialogContent" | "DialogTitle" | "DialogDescription"
        | "DialogClose" | "DialogHeader" | "DialogFooter" | "DialogPortal" | "DialogOverlay" => {
            "dialog"
        }
        "AlertDialog"
        | "AlertDialogTrigger"
        | "AlertDialogContent"
        | "AlertDialogTitle"
        | "AlertDialogDescription"
        | "AlertDialogCancel"
        | "AlertDialogAction"
        | "AlertDialogHeader"
        | "AlertDialogFooter" => "alert-dialog",
        "Popover" | "PopoverTrigger" | "PopoverContent" | "PopoverClose" => "popover",
        "Select" | "SelectTrigger" | "SelectValue" | "SelectContent" | "SelectItem"
        | "SelectGroup" | "SelectLabel" => "select",
        "DropdownMenu"
        | "DropdownMenuTrigger"
        | "DropdownMenuContent"
        | "DropdownMenuItem"
        | "DropdownMenuLabel"
        | "DropdownMenuSeparator" => "dropdown-menu",
        "Card" | "CardHeader" | "CardTitle" | "CardDescription" | "CardContent" | "CardFooter" => {
            "card"
        }
        _ => return None,
    })
}

pub fn intrinsic(tag: &str) -> Option<&str> {
    Some(match tag {
        "Button" => "button",
        "Badge" => "span",
        "Input" => "input",
        "Label" => "label",
        "Separator" => "hr",
        "CardTitle" => "h3",
        "CardDescription" => "p",
        "Card" | "CardHeader" | "CardContent" | "CardFooter" => "div",
        "div" | "section" | "span" | "p" | "h3" | "button" | "input" | "label" | "hr" => tag,
        _ => return None,
    })
}

pub fn validate(
    node: &Node,
    tag: &str,
    attributes: &[Attribute],
    children: &[Node],
) -> Result<(), Diagnostic> {
    let error = |message| diagnostic(node, "native-component", message);
    let Some(kind) = intrinsic(tag) else {
        return Err(error(format!("native component not implemented: {tag}")));
    };
    if ["input", "hr"].contains(&kind) && !children.is_empty() {
        return Err(error(format!("{tag} cannot have children")));
    }
    if (["button", "label"].contains(&kind) || tag == "Badge")
        && children
            .iter()
            .any(|c| !matches!(c.kind, NodeKind::Text(_)))
    {
        return Err(error(format!(
            "{tag}: nested widget composition requires native renderer contract"
        )));
    }
    for attr in attributes {
        let allowed = match attr.name.as_str() {
            "className" | "id" | "aria-label" => true,
            "variant" => ["Button", "Badge"].contains(&tag),
            "size" => tag == "Button",
            "disabled" => ["button", "input"].contains(&kind),
            "readOnly" | "value" | "defaultValue" | "placeholder" => kind == "input",
            "type" => ["input", "button"].contains(&kind),
            "orientation" | "decorative" => tag == "Separator",
            _ => false,
        };
        if !allowed {
            return Err(error(format!(
                "{tag}: native prop not implemented: {}",
                attr.name
            )));
        }
        if ["disabled", "readOnly", "decorative"].contains(&attr.name.as_str()) {
            if !matches!(attr.value, Value::Bool(_) | Value::Prop(_)) {
                return Err(error(format!("{} requires bool", attr.name)));
            }
        } else if !matches!(attr.value, Value::String(_) | Value::Prop(_) | Value::Id(_)) {
            return Err(error(format!("{} requires string", attr.name)));
        }
        let options: Option<&[&str]> = match attr.name.as_str() {
            "variant" if tag == "Button" => Some(&[
                "default",
                "destructive",
                "outline",
                "secondary",
                "ghost",
                "link",
            ]),
            "variant" => Some(&["default", "secondary", "destructive", "outline"]),
            "size" => Some(&["default", "sm", "lg", "icon"]),
            "type" if kind == "input" => Some(&["text"]),
            "type" => Some(&["button"]),
            "orientation" => Some(&["horizontal", "vertical"]),
            _ => None,
        };
        if let (Some(options), Value::String(value)) = (options, &attr.value)
            && !options.contains(&value.as_str())
        {
            return Err(error(format!(
                "{}={value}: native value not implemented",
                attr.name
            )));
        }
    }
    if attributes.iter().any(|a| a.name == "value")
        && attributes.iter().any(|a| a.name == "defaultValue")
    {
        return Err(error("Input cannot combine value and defaultValue".into()));
    }
    if attributes
        .iter()
        .any(|a| a.name == "size" && a.value == Value::String("icon".into()))
        && !attributes.iter().any(|a| {
            a.name == "aria-label"
                && match &a.value {
                    Value::String(value) => !value.trim().is_empty(),
                    Value::Prop(_) => true,
                    _ => false,
                }
        })
    {
        return Err(error("icon Button requires aria-label".into()));
    }
    Ok(())
}

/// Recipes are profile-owned defaults, applied before explicit className declarations.
pub fn recipe(tag: &str, variant: &str, size: &str) -> String {
    let base = match tag {
        "Button" => "inline-flex items-center justify-center rounded-md text-sm font-medium",
        "Badge" => "inline-flex items-center rounded-full text-xs font-semibold px-2 py-1",
        "Input" => {
            "h-9 w-full rounded-md border border-input bg-background text-foreground px-3 py-1 text-sm"
        }
        "Label" => "text-sm font-medium",
        "Card" => "rounded-lg border border-border bg-card text-card-foreground",
        "CardHeader" | "CardContent" | "CardFooter" => "p-6",
        "CardTitle" => "text-xl font-semibold",
        "CardDescription" => "text-sm text-muted-foreground",
        "Separator" => "bg-border",
        _ => "",
    };
    let colors = if ["Button", "Badge"].contains(&tag) {
        match variant {
            "secondary" => "bg-secondary text-secondary-foreground",
            "destructive" => "bg-destructive text-primary-foreground",
            "outline" => "border border-input bg-background text-foreground",
            "ghost" => "bg-background text-foreground",
            "link" => "text-primary",
            _ => "bg-primary text-primary-foreground",
        }
    } else {
        ""
    };
    let sizing = if tag == "Button" {
        match size {
            "sm" => "h-8 px-3",
            "lg" => "h-10 px-6",
            "icon" => "h-9 w-9",
            _ => "h-9 px-4",
        }
    } else {
        ""
    };
    format!("{base} {colors} {sizing}")
}
