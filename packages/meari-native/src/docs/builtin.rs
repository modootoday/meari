//! Library-owned executable fixtures, independent of upstream React implementations.
use crate::{
    application,
    domain::Viewport,
    infrastructure::{artifacts, document::InteractiveDocument, tailwind::TailwindCompiler},
};
use meari::application::Language;
use std::{fs, path::Path};

const CASES: &[(&str, &str)] = &[
    (
        "icons",
        r#"<div className="flex gap-4 items-center"><Icon name="selection-check" size="xs" tone="foreground"/><Icon name="selection-check" size="sm" tone="muted"/><Icon name="selection-check" size="md" tone="primary" stroke="bold"/><Icon name="toggle-thumb" size="lg" tone="destructive" decorative={false} label="Selected"/><Button aria-label="Confirm"><Icon name="selection-check"/>Confirm</Button></div>"#,
    ),
    (
        "use-id",
        r#"<Field><FieldLabel htmlFor={id}>Generated name</FieldLabel><Input id={id}/></Field>"#,
    ),
    (
        "file-input",
        r#"<Field><FieldLabel htmlFor="picture">Picture</FieldLabel><Input id="picture" type="file" accept=".png,.jpg" multiple/><FieldDescription>Choose pictures</FieldDescription></Field>"#,
    ),
    (
        "checkbox-switch",
        r#"<div><Checkbox id="terms"/><Label htmlFor="terms">Accept terms</Label><Switch aria-label="Notifications" defaultChecked/><Checkbox aria-label="Disabled" disabled/></div>"#,
    ),
    (
        "button-group",
        r#"<ButtonGroup aria-label="Actions"><Button>Save</Button><Button disabled>Blocked</Button><ButtonGroupText>Options</ButtonGroupText><Button>Cancel</Button></ButtonGroup>"#,
    ),
    (
        "field",
        r#"<FieldSet><FieldLegend>Account</FieldLegend><FieldGroup><Field><FieldLabel htmlFor="email">Email</FieldLabel><Input id="email" aria-describedby="help error" aria-invalid="true"/><FieldDescription id="help">Required</FieldDescription><FieldError id="error">Invalid email</FieldError></Field></FieldGroup></FieldSet>"#,
    ),
    (
        "tabs",
        r#"<Tabs><TabsList><TabsTrigger value="one">One</TabsTrigger><TabsTrigger value="blocked" disabled>Blocked</TabsTrigger><TabsTrigger value="two">Two</TabsTrigger></TabsList><TabsContent value="one">First</TabsContent><TabsContent value="two">Second</TabsContent></Tabs>"#,
    ),
    (
        "accordion",
        r#"<Accordion><AccordionItem value="one"><AccordionTrigger>One</AccordionTrigger><AccordionContent>First</AccordionContent></AccordionItem><AccordionItem value="two"><AccordionTrigger>Two</AccordionTrigger><AccordionContent>Second</AccordionContent></AccordionItem></Accordion>"#,
    ),
    (
        "dialog",
        r#"<Dialog><DialogTrigger>Open</DialogTrigger><DialogContent><DialogTitle>Dialog</DialogTitle><DialogClose>Close</DialogClose></DialogContent></Dialog>"#,
    ),
    (
        "alert-dialog",
        r#"<AlertDialog><AlertDialogTrigger>Delete</AlertDialogTrigger><AlertDialogContent><AlertDialogTitle>Confirm</AlertDialogTitle><AlertDialogCancel>Cancel</AlertDialogCancel></AlertDialogContent></AlertDialog>"#,
    ),
    (
        "popover",
        r#"<Popover><PopoverTrigger>Open</PopoverTrigger><PopoverContent aria-label="Popup">Popup body</PopoverContent></Popover>"#,
    ),
    (
        "select",
        r#"<Select><SelectTrigger>Select</SelectTrigger><SelectContent><SelectItem value="one">One</SelectItem><SelectItem value="two">Two</SelectItem></SelectContent></Select>"#,
    ),
    (
        "menu",
        r#"<DropdownMenu><DropdownMenuTrigger>Menu</DropdownMenuTrigger><DropdownMenuContent><DropdownMenuItem value="one">One</DropdownMenuItem><DropdownMenuItem value="two">Two</DropdownMenuItem></DropdownMenuContent></DropdownMenu>"#,
    ),
];

fn source(name: &str, body: &str) -> String {
    let (import, hook) = if name == "use-id" {
        ("import {useId} from 'react';", "const id=useId();")
    } else {
        ("", "")
    };
    format!(
        "{import}type Props={{}};export function KitchenSink(props:Props){{{hook}return <div className=\"p-4\">{body}</div>;}}"
    )
}

pub(super) fn run(executable: &Path, output: &Path) -> Result<(), String> {
    fs::create_dir(output).map_err(|e| e.to_string())?;
    let compiler = TailwindCompiler::windows_x64(executable)?;
    let theme = super::theme::reference();
    let mut report = Vec::new();
    for (name, body) in CASES {
        let source = source(name, body);
        let component = meari::parse(&source, Language::Tsx).map_err(|e| e.message)?;
        let css = application::stylesheet(&component, &theme, &compiler)?;
        for (variant, width, dark) in [
            ("wide-light", 960, false),
            ("narrow-light", 390, false),
            ("wide-dark", 960, true),
            ("narrow-dark", 390, true),
        ] {
            let directory = output.join(format!("{name}-{variant}"));
            fs::create_dir(&directory).map_err(|e| e.to_string())?;
            fs::write(directory.join("source.tsx"), &source).map_err(|e| e.to_string())?;
            fs::write(directory.join("compiled.css"), &css).map_err(|e| e.to_string())?;
            let result = (|| {
                let mut doc = InteractiveDocument::new(
                    component.clone(),
                    &css,
                    Viewport {
                        width,
                        height: 480,
                        dark,
                    },
                )?;
                artifacts::capture(&mut doc, &directory, "initial")?;
                let assertions = super::interaction::verify_recorded(&mut doc, Some(&directory))?;
                let (buttons, inputs) = super::controls::verify(&mut doc, Some(&directory))?;
                artifacts::capture(&mut doc, &directory, "final")?;
                Ok::<_, String>(assertions + buttons + inputs)
            })();
            report.push(serde_json::json!({"case":name,"variant":variant,"passed":result.is_ok(),"assertions":result.as_ref().ok(),"error":result.err()}));
            artifacts::json(&output.join("report.json"), &report)?;
        }
    }
    if report.iter().any(|row| row["passed"] != true) {
        Err("library-owned native kitchen-sink assertions failed; inspect library-suite/report.json".into())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn builtin_cases_cover_every_current_native_composite_family() {
        assert_eq!(super::CASES.len(), 13);
        for (name, body) in super::CASES {
            let source = super::source(name, body);
            let component = meari::parse(&source, meari::application::Language::Tsx).unwrap();
            let mut doc = crate::infrastructure::document::InteractiveDocument::new(
                component,
                "[hidden]{display:none!important}",
                crate::domain::Viewport {
                    width: 600,
                    height: 480,
                    dark: false,
                },
            )
            .unwrap();
            let assertions = super::super::interaction::verify(&mut doc).unwrap();
            let (buttons, inputs) = super::super::controls::verify(&mut doc, None).unwrap();
            assert!(assertions + buttons + inputs > 0);
        }
    }
}
