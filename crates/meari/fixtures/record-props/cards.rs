use meari::runtime::*;

#[derive(Clone)]
pub struct CardRow {
    pub key: String,
    pub number: String,
    pub front: String,
    pub back: String,
    pub label: String,
}

impl PropElement for CardRow {
    fn kind() -> meari::domain::PropType {
        meari::domain::PropType::Record(vec![
            meari::domain::Prop {
                name: "key".into(),
                kind: <String as PropElement>::kind(),
            },
            meari::domain::Prop {
                name: "number".into(),
                kind: <String as PropElement>::kind(),
            },
            meari::domain::Prop {
                name: "front".into(),
                kind: <String as PropElement>::kind(),
            },
            meari::domain::Prop {
                name: "back".into(),
                kind: <String as PropElement>::kind(),
            },
            meari::domain::Prop {
                name: "label".into(),
                kind: <String as PropElement>::kind(),
            },
        ])
    }
}

impl IntoValue for CardRow {
    fn into_value(self) -> meari::domain::Value {
        meari::domain::Value::Record(meari::domain::record_value::RecordValue {
            schema: vec![
                meari::domain::Prop {
                    name: "key".into(),
                    kind: <String as PropElement>::kind(),
                },
                meari::domain::Prop {
                    name: "number".into(),
                    kind: <String as PropElement>::kind(),
                },
                meari::domain::Prop {
                    name: "front".into(),
                    kind: <String as PropElement>::kind(),
                },
                meari::domain::Prop {
                    name: "back".into(),
                    kind: <String as PropElement>::kind(),
                },
                meari::domain::Prop {
                    name: "label".into(),
                    kind: <String as PropElement>::kind(),
                },
            ],
            fields: std::collections::BTreeMap::from([
                ("key".into(), self.key.into_value()),
                ("number".into(), self.number.into_value()),
                ("front".into(), self.front.into_value()),
                ("back".into(), self.back.into_value()),
                ("label".into(), self.label.into_value()),
            ]),
        })
    }
}

pub struct Props {
    pub items: Vec<CardRow>,
}

#[allow(non_snake_case)]
pub fn Cards(props: Props) -> Node {
    element(
        "ul",
        vec![],
        vec![repeat_member(
            props.items.clone(),
            "row",
            vec!["key"],
            element(
                "li",
                vec![attribute("data-key", member(local("row"), vec!["key"]))],
                vec![
                    element(
                        "span",
                        vec![],
                        vec![text(member(local("row"), vec!["number"]))],
                    ),
                    element(
                        "span",
                        vec![],
                        vec![text(member(local("row"), vec!["front"]))],
                    ),
                    element(
                        "span",
                        vec![],
                        vec![text(member(local("row"), vec!["back"]))],
                    ),
                    element(
                        "span",
                        vec![],
                        vec![text(member(local("row"), vec!["label"]))],
                    ),
                ],
            ),
        )],
    )
}
