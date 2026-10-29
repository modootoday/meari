use meari::runtime::*;

pub struct Props {
    pub label: String,
    pub disabled: bool,
}

#[allow(non_snake_case)]
pub fn Card(props: Props) -> Node {
    element("section", vec![attribute("className", "flex gap-2")], vec![
        element("button", vec![attribute("disabled", props.disabled)], vec![text(props.label.clone())]),
        element("Badge", vec![attribute("label", props.label.clone())], vec![]),
    ])
}
