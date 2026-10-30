use style::media_queries::MediaType;
use style::servo_arc::Arc as ServoArc;
use style_dom::ElementState;

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use blitz_traits::shell::{ColorScheme, ShellProvider, Viewport};

use crate::{Attribute, BaseDocument, DocumentConfig, ElementData, NodeData, qual_name};

#[test]
fn media_type_defaults_to_screen() {
    let mut document = BaseDocument::new(DocumentConfig::default());
    assert_eq!(*document.media_type(), MediaType::screen());
    assert_eq!(document.stylist_device().media_type(), MediaType::screen());
}

#[test]
fn media_type_honors_config() {
    let mut document = BaseDocument::new(DocumentConfig {
        media_type: Some(MediaType::print()),
        ..Default::default()
    });
    assert_eq!(*document.media_type(), MediaType::print());
    assert_eq!(document.stylist_device().media_type(), MediaType::print());
}

#[test]
fn set_media_type_updates_stylist_device() {
    let mut document = BaseDocument::new(DocumentConfig::default());
    assert_eq!(document.stylist_device().media_type(), MediaType::screen());

    document.set_media_type(MediaType::print());
    assert_eq!(*document.media_type(), MediaType::print());
    assert_eq!(document.stylist_device().media_type(), MediaType::print());
}

#[test]
fn mutator_remove_disabled() {
    let mut document = BaseDocument::new(DocumentConfig::default());
    let id = document.create_node(NodeData::Element(Box::new(ElementData::new(
        qual_name!("button"),
        vec![Attribute {
            name: qual_name!("disabled"),
            value: "".into(),
        }],
    ))));

    let node = document.get_node(id).unwrap();
    assert!(
        node.element_state().contains(ElementState::DISABLED),
        "form node is disabled"
    );
    assert!(
        !node.element_state().contains(ElementState::ENABLED),
        "form node is not enabled yet"
    );

    let mut mutator = document.mutate();
    mutator.clear_attribute(id, qual_name!("disabled"));
    drop(mutator);

    let node = document.get_node(id).unwrap();
    assert!(
        !node.element_state().contains(ElementState::DISABLED),
        "form node is no longer disabled"
    );
    assert!(
        node.element_state().contains(ElementState::ENABLED),
        "form node is enabled"
    );
}

#[test]
fn mutator_set_disabled() {
    let mut document = BaseDocument::new(DocumentConfig::default());
    let id = document.create_node(NodeData::Element(Box::new(ElementData::new(
        qual_name!("button"),
        vec![],
    ))));

    let node = document.get_node(id).unwrap();
    assert!(
        !node.element_state().contains(ElementState::DISABLED),
        "form node is not disabled"
    );
    assert!(
        node.element_state().contains(ElementState::ENABLED),
        "form node is enabled"
    );

    let mut mutator = document.mutate();
    mutator.set_attribute(id, qual_name!("disabled"), "");
    drop(mutator);

    let node = document.get_node(id).unwrap();

    assert!(
        node.element_state().contains(ElementState::DISABLED),
        "form node is disabled"
    );
    assert!(
        !node.element_state().contains(ElementState::ENABLED),
        "form node is no longer enabled enabled"
    );
}

#[test]
fn mutator_set_disabled_invalid_node() {
    let mut document = BaseDocument::new(DocumentConfig::default());
    let id = document.create_node(NodeData::Element(Box::new(ElementData::new(
        qual_name!("a"),
        vec![],
    ))));

    let node = document.get_node(id).unwrap();
    assert!(
        !node.element_state().contains(ElementState::DISABLED),
        "form node is not disabled"
    );
    assert!(
        !node.element_state().contains(ElementState::ENABLED),
        "form node is enabled"
    );

    let mut mutator = document.mutate();
    mutator.set_attribute(id, qual_name!("disabled"), "");
    drop(mutator);

    let node = document.get_node(id).unwrap();
    assert!(
        !node.element_state().contains(ElementState::DISABLED),
        "form node is not disabled"
    );
    assert!(
        !node.element_state().contains(ElementState::ENABLED),
        "form node is enabled"
    );
}

#[test]
fn mutator_id_attribute_updates_id_map() {
    let mut document = BaseDocument::new(DocumentConfig::default());
    let root_id = document.root_node().id;

    let node_id = {
        let mut mutator = document.mutate();
        let node_id = mutator.create_element(
            qual_name!("div"),
            vec![Attribute {
                name: qual_name!("id"),
                value: "old".into(),
            }],
        );
        mutator.append_children(root_id, &[node_id]);
        node_id
    };
    assert_eq!(document.get_element_by_id("old"), Some(node_id));

    {
        let mut mutator = document.mutate();
        mutator.set_attribute(node_id, qual_name!("id"), "new");
    }
    assert_eq!(document.get_element_by_id("new"), Some(node_id));
    assert_eq!(document.get_element_by_id("old"), None);

    {
        let mut mutator = document.mutate();
        mutator.clear_attribute(node_id, qual_name!("id"));
    }
    assert_eq!(document.get_element_by_id("new"), None);
}

#[test]
fn get_element_by_id_duplicate_ids_first_in_tree_order_wins() {
    let mut document = BaseDocument::new(DocumentConfig::default());
    let root_id = document.root_node().id;

    let (first_id, second_id) = {
        let mut mutator = document.mutate();
        let first_id = mutator.create_element(qual_name!("div"), vec![]);
        let second_id = mutator.create_element(qual_name!("div"), vec![]);
        mutator.append_children(root_id, &[first_id, second_id]);
        // Assign the id to the later node first so that insertion order
        // differs from tree order
        mutator.set_attribute(second_id, qual_name!("id"), "dup");
        mutator.set_attribute(first_id, qual_name!("id"), "dup");
        (first_id, second_id)
    };
    assert_eq!(document.get_element_by_id("dup"), Some(first_id));

    {
        let mut mutator = document.mutate();
        mutator.remove_node(first_id);
    }
    assert_eq!(document.get_element_by_id("dup"), Some(second_id));
}

#[derive(Default)]
struct RedrawShell {
    redraw_requests: AtomicUsize,
}

impl ShellProvider for RedrawShell {
    fn request_redraw(&self) {
        self.redraw_requests.fetch_add(1, Ordering::Relaxed);
    }
}

#[test]
fn mutator_requests_redraw_only_after_mutation() {
    let shell = Arc::new(RedrawShell::default());
    let mut document = BaseDocument::new(DocumentConfig {
        shell_provider: Some(shell.clone()),
        ..Default::default()
    });
    let root_id = document.root_node().id;

    {
        let mut mutator = document.mutate();
        let parent_id = mutator.create_element(qual_name!("div"), vec![]);
        let child_id = mutator.create_element(qual_name!("span"), vec![]);
        mutator.append_children(parent_id, &[child_id]);
        mutator.remove_and_drop_all_children(parent_id);
        mutator.set_attribute(parent_id, qual_name!("id"), "detached");
    }
    assert_eq!(shell.redraw_requests.load(Ordering::Relaxed), 0);

    {
        let mutator = document.mutate();
        assert_eq!(mutator.child_ids(root_id).len(), 0);
    }

    {
        let mut mutator = document.mutate();
        let node_id = mutator.create_element(qual_name!("div"), vec![]);
        mutator.append_children(root_id, &[node_id]);
        mutator.set_attribute(node_id, qual_name!("id"), "in-document");
    }
    assert_eq!(shell.redraw_requests.load(Ordering::Relaxed), 1);

    {
        let mut mutator = document.mutate();
        let parent_id = mutator.create_element(qual_name!("div"), vec![]);
        let child_id = mutator.create_element(qual_name!("span"), vec![]);
        mutator.append_children(root_id, &[parent_id]);
        mutator.append_children(parent_id, &[child_id]);
        mutator.remove_and_drop_all_children(parent_id);
    }
    assert_eq!(shell.redraw_requests.load(Ordering::Relaxed), 2);

    {
        let mut mutator = document.mutate();
        let parent_id = mutator.create_element(qual_name!("div"), vec![]);
        let child_id = mutator.create_element(qual_name!("span"), vec![]);
        let detached_target_id = mutator.create_element(qual_name!("div"), vec![]);
        mutator.append_children(root_id, &[parent_id]);
        mutator.append_children(parent_id, &[child_id]);
        assert_eq!(shell.redraw_requests.load(Ordering::Relaxed), 2);
        mutator.append_children(detached_target_id, &[child_id]);
    }
    assert_eq!(shell.redraw_requests.load(Ordering::Relaxed), 3);
}

#[test]
fn moving_subtree_out_of_document_clears_in_document_flag() {
    let shell = Arc::new(RedrawShell::default());
    let mut document = BaseDocument::new(DocumentConfig {
        shell_provider: Some(shell.clone()),
        ..Default::default()
    });
    let root_id = document.root_node().id;
    let (child_id, grandchild_id, detached_parent_id) = {
        let mut mutator = document.mutate();
        let in_document_parent_id = mutator.create_element(qual_name!("div"), vec![]);
        let child_id = mutator.create_element(qual_name!("div"), vec![]);
        let grandchild_id = mutator.create_element(qual_name!("span"), vec![]);
        let detached_parent_id = mutator.create_element(qual_name!("section"), vec![]);
        mutator.append_children(root_id, &[in_document_parent_id]);
        mutator.append_children(in_document_parent_id, &[child_id]);
        mutator.append_children(child_id, &[grandchild_id]);
        (child_id, grandchild_id, detached_parent_id)
    };
    assert_eq!(shell.redraw_requests.load(Ordering::Relaxed), 1);
    assert!(document.get_node(child_id).unwrap().flags.is_in_document());
    assert!(
        document
            .get_node(grandchild_id)
            .unwrap()
            .flags
            .is_in_document()
    );

    {
        let mut mutator = document.mutate();
        mutator.append_children(detached_parent_id, &[child_id]);
    }
    assert_eq!(shell.redraw_requests.load(Ordering::Relaxed), 2);
    assert!(!document.get_node(child_id).unwrap().flags.is_in_document());
    assert!(
        !document
            .get_node(grandchild_id)
            .unwrap()
            .flags
            .is_in_document()
    );

    {
        let mut mutator = document.mutate();
        mutator.set_attribute(child_id, qual_name!("id"), "detached");
    }
    assert_eq!(shell.redraw_requests.load(Ordering::Relaxed), 2);

    {
        let mut mutator = document.mutate();
        mutator.append_children(root_id, &[child_id]);
    }
    assert_eq!(shell.redraw_requests.load(Ordering::Relaxed), 3);
    assert!(document.get_node(child_id).unwrap().flags.is_in_document());
    assert!(
        document
            .get_node(grandchild_id)
            .unwrap()
            .flags
            .is_in_document()
    );
}

#[test]
fn style_property_updates_nested_layout() {
    let mut document = BaseDocument::new(DocumentConfig {
        viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
        ..Default::default()
    });
    let root_id = document.root_node().id;

    let mover_id = {
        let mut mutator = document.mutate();
        let parent_id = mutator.create_element(qual_name!("div"), vec![]);
        let mover_id = mutator.create_element(qual_name!("div"), vec![]);
        mutator.set_style_property(parent_id, "position", "relative");
        mutator.set_style_property(parent_id, "width", "800px");
        mutator.set_style_property(parent_id, "height", "600px");
        mutator.set_style_property(mover_id, "position", "absolute");
        mutator.set_style_property(mover_id, "left", "0px");
        mutator.set_style_property(mover_id, "top", "0px");
        mutator.append_children(parent_id, &[mover_id]);
        mutator.append_children(root_id, &[parent_id]);
        mover_id
    };

    document.resolve(0.0);
    assert_eq!(
        document
            .get_node(mover_id)
            .unwrap()
            .final_layout()
            .location
            .x,
        0.0
    );

    {
        let mut mutator = document.mutate();
        mutator.set_style_property(mover_id, "left", "120px");
    }

    document.resolve(0.0);
    assert_eq!(
        document
            .get_node(mover_id)
            .unwrap()
            .final_layout()
            .location
            .x,
        120.0
    );
}

#[test]
fn style_property_mutation_copies_blocks_in_rule_tree() {
    let mut document = BaseDocument::new(DocumentConfig {
        viewport: Some(Viewport::new(800, 600, 1.0, ColorScheme::Light)),
        ..Default::default()
    });
    let root_id = document.root_node().id;

    let div_id = {
        let mut mutator = document.mutate();
        let div_id = mutator.create_element(qual_name!("div"), vec![]);
        mutator.set_style_property(div_id, "width", "100px");
        mutator.set_style_property(div_id, "height", "50px");
        mutator.append_children(root_id, &[div_id]);
        div_id
    };

    let style_block = |document: &BaseDocument| {
        document
            .get_node(div_id)
            .unwrap()
            .element_data()
            .unwrap()
            .style_attribute
            .clone()
            .unwrap()
    };

    let before_resolve = style_block(&document);
    {
        let mut mutator = document.mutate();
        mutator.set_style_property(div_id, "height", "60px");
    }
    // Not yet in the rule tree: mutated in place.
    assert!(ServoArc::ptr_eq(&before_resolve, &style_block(&document)));

    document.resolve(0.0);
    let in_rule_tree = style_block(&document);
    assert!(
        in_rule_tree
            .read_with(&document.guard.read())
            .immutable
            .load(Ordering::Relaxed)
    );

    {
        let mut mutator = document.mutate();
        mutator.set_style_property(div_id, "width", "200px");
    }
    let after_set = style_block(&document);
    assert!(!ServoArc::ptr_eq(&in_rule_tree, &after_set));
    {
        let guard = document.guard.read();
        let old = in_rule_tree.read_with(&guard);
        let new = after_set.read_with(&guard);
        assert_eq!(old.declarations().len(), 2);
        assert_eq!(new.declarations().len(), 2);
        assert_ne!(old, new);
    }

    document.resolve(0.0);
    assert_eq!(
        document.get_node(div_id).unwrap().final_layout().size.width,
        200.0
    );

    let in_rule_tree = style_block(&document);
    {
        let mut mutator = document.mutate();
        mutator.remove_style_property(div_id, "height");
    }
    let after_remove = style_block(&document);
    assert!(!ServoArc::ptr_eq(&in_rule_tree, &after_remove));
    {
        let guard = document.guard.read();
        assert_eq!(in_rule_tree.read_with(&guard).declarations().len(), 2);
        assert_eq!(after_remove.read_with(&guard).declarations().len(), 1);
    }
}
