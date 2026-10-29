use super::*;

fn application(body: &str) -> ApplicationDefinition {
    let mut initial = definition();
    initial.component = meari::parse(
        &format!("type Props={{}};export function Counter(props:Props){{return {body};}}"),
        Language::Tsx,
    )
    .unwrap();
    initial
}

#[test]
fn retained_reload_preserves_latest_interaction_for_every_native_composite_family() {
    for body in [
        "<Tabs defaultValue=\"one\"><TabsList><TabsTrigger value=\"one\">One</TabsTrigger><TabsTrigger id=\"trigger\" value=\"two\">Before</TabsTrigger></TabsList><TabsContent value=\"one\">First</TabsContent><TabsContent value=\"two\">Second</TabsContent></Tabs>",
        "<Accordion><AccordionItem value=\"one\"><AccordionTrigger id=\"trigger\">Before</AccordionTrigger><AccordionContent>Panel</AccordionContent></AccordionItem></Accordion>",
        "<Dialog><DialogTrigger id=\"trigger\">Before</DialogTrigger><DialogContent><DialogTitle>Title</DialogTitle><DialogClose>Close</DialogClose></DialogContent></Dialog>",
        "<AlertDialog><AlertDialogTrigger id=\"trigger\">Before</AlertDialogTrigger><AlertDialogContent><AlertDialogTitle>Title</AlertDialogTitle><AlertDialogCancel>Cancel</AlertDialogCancel></AlertDialogContent></AlertDialog>",
        "<Popover><PopoverTrigger id=\"trigger\">Before</PopoverTrigger><PopoverContent>Panel</PopoverContent></Popover>",
        "<Select><SelectTrigger id=\"trigger\">Before</SelectTrigger><SelectContent><SelectItem value=\"one\">One</SelectItem><SelectItem value=\"two\">Two</SelectItem></SelectContent></Select>",
        "<DropdownMenu><DropdownMenuTrigger id=\"trigger\">Before</DropdownMenuTrigger><DropdownMenuContent><DropdownMenuItem>One</DropdownMenuItem><DropdownMenuItem>Two</DropdownMenuItem></DropdownMenuContent></DropdownMenu>",
    ] {
        let initial = application(body);
        let mut document =
            InteractiveDocument::from_application(&initial, "[hidden]{display:none!important}")
                .unwrap();
        let trigger = document
            .elements()
            .iter()
            .find(|element| element.id == "trigger")
            .unwrap()
            .path
            .clone();
        let id = document
            .renderer
            .document
            .get_element_by_id("trigger")
            .unwrap();
        let mut next = application(&body.replace("Before", "After"));
        next.stamp.generation += 1;
        next.stamp.graph_hash = ContentHash::of(body.as_bytes());
        let prepared = document
            .prepare_application_replacement(next.clone())
            .unwrap();
        document
            .dispatch(Action::Activate { path: trigger })
            .unwrap();
        let live = serde_json::to_value(&document.session).unwrap();
        let committed = document
            .commit_reversible_application_replacement(prepared)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&document.session).unwrap(),
            live,
            "{body}"
        );
        assert_eq!(
            document.renderer.document.get_element_by_id("trigger"),
            Some(id)
        );
        assert_eq!(
            document
                .renderer
                .document
                .get_node(id)
                .unwrap()
                .text_content(),
            "After"
        );
        assert_eq!(document.application_stamp(), Some(&next.stamp));
        document
            .rollback_application_replacement(committed)
            .unwrap();
        assert_eq!(
            serde_json::to_value(&document.session).unwrap(),
            live,
            "rollback {body}"
        );
        assert_eq!(
            document.renderer.document.get_element_by_id("trigger"),
            Some(id)
        );
        assert_eq!(
            document
                .renderer
                .document
                .get_node(id)
                .unwrap()
                .text_content(),
            "Before"
        );
        assert_eq!(document.application_stamp(), Some(&initial.stamp));
    }
}

#[test]
fn incompatible_composite_layout_and_controlled_updates_keep_the_current_document() {
    let body = "<Tabs value=\"one\"><TabsList><TabsTrigger value=\"one\">One</TabsTrigger><TabsTrigger id=\"trigger\" value=\"two\">Before</TabsTrigger></TabsList><TabsContent value=\"one\">First</TabsContent><TabsContent value=\"two\">Second</TabsContent></Tabs>";
    let initial = application(body);
    let document = InteractiveDocument::from_application(&initial, "").unwrap();
    let current = serde_json::to_value(&document.session).unwrap();
    for changed in [
        body.replacen("value=\"one\"", "value=\"two\"", 1),
        body.replace(
            "id=\"trigger\" value=\"two\"",
            "id=\"trigger\" value=\"two\" disabled",
        ),
        body.replace("<Tabs value", "<Tabs orientation=\"vertical\" value"),
        body.replace("value=\"two\"", "value=\"renamed\""),
        body.replace(
            "<TabsContent value=\"two\">",
            "<TabsContent value=\"renamed\">",
        ),
    ] {
        let mut next = application(&changed);
        next.stamp.generation += 1;
        assert!(document.prepare_application_replacement(next).is_err());
        assert_eq!(serde_json::to_value(&document.session).unwrap(), current);
        assert_eq!(document.application_stamp(), Some(&initial.stamp));
    }
}
