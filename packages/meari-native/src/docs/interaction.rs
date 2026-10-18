//! Native contract assertions. These do not stand in for upstream-provider/OS acceptance.
use crate::{
    domain::widgets::{Action, Family},
    infrastructure::document::InteractiveDocument,
};

struct Trace<'a> {
    directory: Option<&'a std::path::Path>,
    index: usize,
    frames: Vec<serde_json::Value>,
}

fn dispatch(
    doc: &mut InteractiveDocument,
    trace: &mut Trace<'_>,
    action: Action,
) -> Result<bool, String> {
    let action_json = serde_json::to_value(&action).map_err(|e| e.to_string())?;
    let changed = doc.dispatch(action)?;
    if let Some(directory) = trace.directory {
        let name = format!("state-{:04}", trace.index);
        let capture = crate::infrastructure::artifacts::capture(doc, directory, &name)?;
        trace.frames.push(
            serde_json::json!({"action": action_json, "changed": changed, "capture": capture}),
        );
        trace.index += 1;
        // Keep partial traces reviewable even if a later oracle assertion fails.
        crate::infrastructure::artifacts::json(&directory.join("states.json"), &trace.frames)?;
    }
    Ok(changed)
}

fn require(value: bool, message: impl Into<String>) -> Result<(), String> {
    if value { Ok(()) } else { Err(message.into()) }
}

fn attribute<'a>(doc: &'a InteractiveDocument, path: &str, name: &str) -> Option<&'a str> {
    doc.elements()
        .iter()
        .find(|e| e.path == path)?
        .attributes
        .get(name)
        .map(String::as_str)
}

fn focused(doc: &InteractiveDocument, path: &str) -> Result<(), String> {
    let expected = doc
        .path_id(path)
        .and_then(|id| doc.renderer.document.get_element_by_id(id))
        .ok_or("missing focus target")?;
    require(
        doc.renderer.document.get_focussed_node_id() == Some(expected),
        format!("native focus mismatch: {path}"),
    )
}

fn panel(
    doc: &InteractiveDocument,
    trigger: &str,
    selected: bool,
    tabs: bool,
) -> Result<usize, String> {
    let state = if tabs {
        "aria-selected"
    } else {
        "aria-expanded"
    };
    require(
        attribute(doc, trigger, state) == Some(if selected { "true" } else { "false" }),
        format!("{state}: {trigger}"),
    )?;
    let Some(id) = attribute(doc, trigger, "aria-controls") else {
        let root = doc
            .session
            .owner(trigger)
            .map(|widget| widget.root.as_str());
        let has_panels = doc.elements().iter().any(|element| {
            element.attributes.get("role").map(String::as_str) == Some("tabpanel")
                && doc
                    .session
                    .owner(&element.path)
                    .map(|widget| widget.root.as_str())
                    == root
        });
        require(
            tabs && !has_panels,
            format!("missing aria-controls: {trigger}"),
        )?;
        return Ok(2); // Official tab-strip styling examples intentionally contain no panels.
    };
    let panel = doc
        .elements()
        .iter()
        .find(|e| e.id == id)
        .ok_or("aria-controls target missing")?;
    require(
        panel.attributes.contains_key("hidden") != selected,
        "panel visibility disagrees with selection",
    )?;
    let trigger_id = doc.path_id(trigger).ok_or("trigger id missing")?;
    require(
        panel.attributes.get("aria-labelledby").map(String::as_str) == Some(trigger_id),
        "panel aria-labelledby mismatch",
    )?;
    Ok(4)
}

#[cfg(test)]
pub(super) fn verify(doc: &mut InteractiveDocument) -> Result<usize, String> {
    verify_recorded(doc, None)
}

pub(super) fn verify_recorded(
    doc: &mut InteractiveDocument,
    directory: Option<&std::path::Path>,
) -> Result<usize, String> {
    let mut trace = Trace {
        directory,
        index: 0,
        frames: Vec::new(),
    };
    let widgets: Vec<_> = doc.session.widgets.values().cloned().collect();
    let mut assertions = 0;
    for widget in widgets {
        // An ancestor's hidden panel requires its own scenario before this widget is reachable.
        if doc.elements().iter().any(|e| {
            widget.root.starts_with(&format!("{}/", e.path)) && e.attributes.contains_key("hidden")
        }) {
            return Err(format!(
                "nested widget needs an explicit reachability scenario: {}",
                widget.root
            ));
        }
        for item in widget.items.iter().filter(|i| i.disabled) {
            let before = serde_json::to_value(&doc.session).map_err(|e| e.to_string())?;
            require(
                !dispatch(
                    doc,
                    &mut trace,
                    Action::Activate {
                        path: item.path.clone(),
                    },
                )?,
                "disabled item activated",
            )?;
            require(
                before == serde_json::to_value(&doc.session).map_err(|e| e.to_string())?,
                "disabled item changed state",
            )?;
            require(
                attribute(doc, &item.path, "aria-disabled") == Some("true"),
                "disabled item missing aria-disabled",
            )?;
            assertions += 3;
        }
        let enabled: Vec<_> = widget
            .items
            .iter()
            .filter(|i| !i.disabled)
            .cloned()
            .collect();
        match widget.family {
            Family::Tabs | Family::Accordion => {
                let tabs = widget.family == Family::Tabs;
                for item in &enabled {
                    if !tabs && doc.session.widgets[&widget.root].value == item.value {
                        dispatch(
                            doc,
                            &mut trace,
                            Action::Activate {
                                path: item.path.clone(),
                            },
                        )?;
                    }
                    dispatch(
                        doc,
                        &mut trace,
                        Action::Activate {
                            path: item.path.clone(),
                        },
                    )?;
                    require(
                        doc.session.widgets[&widget.root].value == item.value,
                        "item did not select expected value",
                    )?;
                    focused(doc, &item.path)?;
                    assertions += 2 + panel(doc, &item.path, true, tabs)?;
                    if !tabs {
                        dispatch(
                            doc,
                            &mut trace,
                            Action::Activate {
                                path: item.path.clone(),
                            },
                        )?;
                        assertions += panel(doc, &item.path, false, false)?;
                    }
                }
                if let (Some(first), Some(last)) = (enabled.first(), enabled.last()) {
                    dispatch(
                        doc,
                        &mut trace,
                        Action::Key {
                            path: first.path.clone(),
                            key: "End".into(),
                            shift: false,
                        },
                    )?;
                    focused(doc, &last.path)?;
                    dispatch(
                        doc,
                        &mut trace,
                        Action::Key {
                            path: last.path.clone(),
                            key: "Home".into(),
                            shift: false,
                        },
                    )?;
                    focused(doc, &first.path)?;
                    let next = enabled.get(1).unwrap_or(first);
                    let key = if widget.vertical {
                        "ArrowDown"
                    } else {
                        "ArrowRight"
                    };
                    dispatch(
                        doc,
                        &mut trace,
                        Action::Key {
                            path: first.path.clone(),
                            key: key.into(),
                            shift: false,
                        },
                    )?;
                    focused(doc, &next.path)?;
                    if tabs {
                        assertions += panel(doc, &next.path, true, true)?;
                    }
                    assertions += 3;
                }
            }
            Family::Dialog
            | Family::AlertDialog
            | Family::Popover
            | Family::Select
            | Family::Menu => {
                let trigger = widget.trigger.as_ref().ok_or("overlay trigger missing")?;
                if doc.session.disabled_paths.contains(trigger) {
                    let before = serde_json::to_value(&doc.session).map_err(|e| e.to_string())?;
                    require(
                        !dispatch(
                            doc,
                            &mut trace,
                            Action::Activate {
                                path: trigger.clone(),
                            },
                        )?,
                        "disabled overlay trigger activated",
                    )?;
                    require(
                        before == serde_json::to_value(&doc.session).map_err(|e| e.to_string())?,
                        "disabled overlay state changed",
                    )?;
                    assertions += 2;
                    continue;
                }
                if doc.session.widgets[&widget.root].open {
                    dispatch(
                        doc,
                        &mut trace,
                        Action::Activate {
                            path: trigger.clone(),
                        },
                    )?;
                }
                dispatch(
                    doc,
                    &mut trace,
                    Action::Activate {
                        path: trigger.clone(),
                    },
                )?;
                require(
                    doc.session.widgets[&widget.root].open,
                    "overlay did not open",
                )?;
                require(
                    attribute(doc, trigger, "aria-expanded") == Some("true"),
                    "overlay trigger aria-expanded mismatch",
                )?;
                let content = widget.content.as_ref().ok_or("overlay content missing")?;
                require(
                    attribute(doc, content, "hidden").is_none(),
                    "open overlay is hidden",
                )?;
                assertions += 3;
                if let Some(item) = enabled
                    .first()
                    .filter(|_| matches!(widget.family, Family::Select | Family::Menu))
                {
                    dispatch(
                        doc,
                        &mut trace,
                        Action::Activate {
                            path: item.path.clone(),
                        },
                    )?;
                    require(
                        doc.session.widgets[&widget.root].value == item.value,
                        "overlay item selection mismatch",
                    )?;
                    require(
                        !doc.session.widgets[&widget.root].open,
                        "selection did not dismiss overlay",
                    )?;
                    focused(doc, trigger)?;
                    assertions += 3;
                } else if widget.family == Family::AlertDialog {
                    dispatch(
                        doc,
                        &mut trace,
                        Action::Key {
                            path: content.clone(),
                            key: "Escape".into(),
                            shift: false,
                        },
                    )?;
                    require(
                        doc.session.widgets[&widget.root].open,
                        "alert dialog dismissed on Escape",
                    )?;
                    let close = widget
                        .close_paths
                        .first()
                        .ok_or("alert dialog requires an explicit close scenario")?;
                    dispatch(
                        doc,
                        &mut trace,
                        Action::Activate {
                            path: close.clone(),
                        },
                    )?;
                    require(
                        !doc.session.widgets[&widget.root].open,
                        "alert dialog close did not dismiss",
                    )?;
                    focused(doc, trigger)?;
                    assertions += 3;
                } else {
                    dispatch(
                        doc,
                        &mut trace,
                        Action::Key {
                            path: content.clone(),
                            key: "Escape".into(),
                            shift: false,
                        },
                    )?;
                    require(
                        !doc.session.widgets[&widget.root].open,
                        "Escape did not dismiss overlay",
                    )?;
                    focused(doc, trigger)?;
                    assertions += 2;
                }
            }
        }
    }
    if let Some(directory) = trace.directory {
        crate::infrastructure::artifacts::json(&directory.join("states.json"), &trace.frames)?;
    }
    Ok(assertions)
}

#[cfg(test)]
mod tests {
    #[test]
    fn action_trace_links_each_dispatched_state_to_native_artifacts() {
        let output = std::env::temp_dir().join(format!(
            "bridge-state-trace-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&output).unwrap();
        let source = meari::parse("type Props={};export function Demo(props:Props){return <Tabs><TabsList><TabsTrigger value=\"one\">One</TabsTrigger><TabsTrigger value=\"two\">Two</TabsTrigger></TabsList><TabsContent value=\"one\">First</TabsContent><TabsContent value=\"two\">Second</TabsContent></Tabs>;}",meari::application::Language::Tsx).unwrap();
        let mut doc = crate::infrastructure::document::InteractiveDocument::new(
            source,
            "[hidden]{display:none!important}",
            crate::domain::Viewport {
                width: 400,
                height: 300,
                dark: false,
            },
        )
        .unwrap();
        super::verify_recorded(&mut doc, Some(&output)).unwrap();
        let frames: Vec<serde_json::Value> =
            serde_json::from_slice(&std::fs::read(output.join("states.json")).unwrap()).unwrap();
        assert_eq!(frames.len(), 5);
        for frame in frames {
            assert!(frame["action"].is_object());
            let name = frame["capture"]["name"].as_str().unwrap();
            for suffix in ["png", "layout.json", "a11y.json"] {
                assert!(output.join(format!("{name}.{suffix}")).is_file());
            }
        }
    }
    #[test]
    fn native_oracle_asserts_multiline_tabs_and_disabled_accordion_behavior() {
        use crate::{domain::Viewport, infrastructure::document::InteractiveDocument};
        for body in [
            "<Tabs><TabsList><TabsTrigger value=\"one\">One</TabsTrigger><TabsTrigger value=\"disabled\" disabled>Disabled</TabsTrigger><TabsTrigger value=\"two\">Two</TabsTrigger></TabsList><TabsContent value=\"one\">First\n panel</TabsContent><TabsContent value=\"two\">Second</TabsContent></Tabs>",
            "<Accordion><AccordionItem value=\"one\"><AccordionTrigger>One</AccordionTrigger><AccordionContent>First\n panel</AccordionContent></AccordionItem><AccordionItem value=\"disabled\" disabled><AccordionTrigger>Disabled</AccordionTrigger><AccordionContent>Disabled</AccordionContent></AccordionItem></Accordion>",
            "<Tabs><TabsList><TabsTrigger value=\"one\">One</TabsTrigger><TabsTrigger value=\"two\">Two</TabsTrigger></TabsList></Tabs>",
        ] {
            let source = meari::parse(
                &format!("type Props={{}};export function Demo(props:Props){{return {body};}}"),
                meari::application::Language::Tsx,
            )
            .unwrap();
            let mut doc = InteractiveDocument::new(
                source,
                "[hidden]{display:none!important}",
                Viewport {
                    width: 800,
                    height: 600,
                    dark: false,
                },
            )
            .unwrap();
            assert!(super::verify(&mut doc).unwrap() > 8);
        }
    }

    #[test]
    fn partially_missing_tab_panels_are_not_treated_as_panel_free_examples() {
        let source=meari::parse("type Props={};export function Demo(props:Props){return <Tabs><TabsList><TabsTrigger value=\"one\">One</TabsTrigger><TabsTrigger value=\"two\">Two</TabsTrigger></TabsList><TabsContent value=\"one\">First</TabsContent></Tabs>;}",meari::application::Language::Tsx).unwrap();
        let mut doc = crate::infrastructure::document::InteractiveDocument::new(
            source,
            "[hidden]{display:none!important}",
            crate::domain::Viewport {
                width: 800,
                height: 600,
                dark: false,
            },
        )
        .unwrap();
        assert!(
            super::verify(&mut doc)
                .unwrap_err()
                .contains("missing aria-controls")
        );
    }
}
