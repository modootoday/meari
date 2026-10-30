use style_dom::ElementState;

use crate::{Attribute, BaseDocument, DocumentConfig, ElementData, NodeData, qual_name};

#[test]
fn create_node_with_disabled_attr() {
    let mut document = BaseDocument::new(DocumentConfig::default());
    let node = document.create_node(NodeData::Element(Box::new(ElementData::new(
        qual_name!("button"),
        vec![Attribute {
            name: qual_name!("disabled"),
            value: "".into(),
        }],
    ))));
    let node = document.get_node(node).unwrap();

    assert!(
        node.element_state().contains(ElementState::DISABLED),
        "form node is disabled"
    );
    assert!(
        !node.element_state().contains(ElementState::ENABLED),
        "form node is not enabled"
    );
}

#[test]
fn ignore_disabled_attr_content() {
    let mut document = BaseDocument::new(DocumentConfig::default());
    let node = document.create_node(NodeData::Element(Box::new(ElementData::new(
        qual_name!("button"),
        vec![Attribute {
            name: qual_name!("disabled"),
            value: "false".into(),
        }],
    ))));
    let node = document.get_node(node).unwrap();

    assert!(
        node.element_state().contains(ElementState::DISABLED),
        "form node is disabled"
    );
    assert!(
        !node.element_state().contains(ElementState::ENABLED),
        "form node is not enabled"
    );
}

#[test]
fn create_node_with_ignored_disable() {
    let mut document = BaseDocument::new(DocumentConfig::default());
    let node = document.create_node(NodeData::Element(Box::new(ElementData::new(
        qual_name!("a"),
        vec![Attribute {
            name: qual_name!("disabled"),
            value: "".into(),
        }],
    ))));
    let node = document.get_node(node).unwrap();

    assert!(
        !node.element_state().contains(ElementState::DISABLED),
        "Non form node cannot be disabled"
    );
    assert!(
        !node.element_state().contains(ElementState::ENABLED),
        "Non form node cannot be enabled"
    );
}

#[test]
fn create_empty_enabled_node() {
    let mut document = BaseDocument::new(DocumentConfig::default());
    let node = document.create_node(NodeData::Element(Box::new(ElementData::new(
        qual_name!("button"),
        vec![],
    ))));
    let node = document.get_node(node).unwrap();

    assert!(
        node.element_state().contains(ElementState::ENABLED),
        "Button should be enabled by default"
    );
}
