use meari::{
    application::{Language, prepare_native},
    domain::{
        design::{native::Context, theme::*, utilities::*},
        *,
    },
};
use std::collections::BTreeMap;

fn component(body: &str) -> Component {
    meari::parse(&format!("type Props = {{ label: string; disabled: boolean; }}; export function Demo(props: Props) {{ return {body}; }}"), Language::Tsx).unwrap()
}

fn theme() -> Theme {
    Theme {
        light: COLORS
            .iter()
            .enumerate()
            .map(|(i, name)| ((*name).into(), [i as u8, 20, 30, 255]))
            .collect(),
        dark: COLORS
            .iter()
            .enumerate()
            .map(|(i, name)| ((*name).into(), [i as u8, 80, 90, 255]))
            .collect(),
        ..Theme::default()
    }
}

#[test]
fn theme_scales_and_invalid_numeric_inputs_are_checked() {
    let mut theme = theme();
    theme.spacing_rem = 0.5;
    theme.radius_rem = 1.0;
    let rules = parse("p-2 rounded-md").unwrap();
    let resolved = resolve(&rules, &theme, env(800.0, false), Interaction::default()).unwrap();
    assert_eq!(resolved[&Property::PaddingLeft], ResolvedValue::Px(16.0));
    assert_eq!(resolved[&Property::Radius], ResolvedValue::Px(12.8));
    theme.spacing_rem = f32::INFINITY;
    assert!(resolve(&rules, &theme, env(800.0, false), Interaction::default()).is_err());
}

fn env(width: f32, dark: bool) -> Environment {
    Environment {
        viewport_width: width,
        root_font_px: 16.0,
        dark,
    }
}

#[test]
fn shadcn_imports_and_classes_roundtrip_in_both_directions() {
    let ir = component(
        r#"<Card className="flex flex-col gap-4 p-6 bg-card text-card-foreground"><CardHeader><CardTitle>{props.label}</CardTitle></CardHeader><CardContent><Button variant="outline" size="sm" disabled={props.disabled} className="md:px-6 hover:bg-accent">{props.label}</Button><Badge variant="secondary">New</Badge><Input placeholder="검색" readOnly={true} value={props.label}/><Separator orientation="horizontal"/><Label>설명</Label></CardContent><CardFooter>끝</CardFooter></Card>"#,
    );
    for language in [Language::Rust, Language::Tsx] {
        let output = meari::emit(&ir, language).unwrap();
        assert!(
            output.design_system.native_syntax_supported,
            "{:?}",
            output.design_system.diagnostics
        );
        assert!(!output.design_system.native_renderer_verified);
        let parsed = meari::parse(&output.code, language).unwrap();
        assert!(ir.semantic_eq(&parsed), "{}", output.code);
        if matches!(language, Language::Tsx) {
            assert!(
                output
                    .code
                    .contains("import { Button } from \"@/components/ui/button\";")
            );
            assert_eq!(output.code.matches("@/components/ui/card").count(), 1);
            for mapping in output.mappings {
                assert!(
                    output
                        .code
                        .get(mapping.generated.start..mapping.generated.end)
                        .is_some()
                );
            }
        }
    }
}

#[test]
fn unknown_native_expressions_are_preserved_and_reported() {
    for classes in [
        "grid grid-cols-[1fr_2fr]",
        "group-hover:bg-primary",
        "bg-[#ff0000]",
        "p-2 px-4",
        "hover:bg-primary dark:bg-secondary",
    ] {
        let ir = component(&format!(
            "<div className=\"{classes}\">{{props.label}}</div>"
        ));
        let out = meari::emit(&ir, Language::Rust).unwrap();
        assert!(!out.design_system.native_syntax_supported, "{classes}");
        assert_eq!(out.design_system.diagnostics[0].code, "native-style");
        assert!(
            out.design_system.diagnostics[0].span.end > out.design_system.diagnostics[0].span.start
        );
        assert!(ir.semantic_eq(&meari::parse(&out.code, Language::Rust).unwrap()));
    }
}

#[test]
fn utility_values_and_breakpoint_boundaries_are_explicit() {
    let rules =
        parse("p-2 md:p-4 lg:p-6 w-full h-9 gap-3 text-sm border-2 rounded-md opacity-50").unwrap();
    for (width, padding) in [
        (767.99, 8.0),
        (768.0, 16.0),
        (768.01, 16.0),
        (1023.99, 16.0),
        (1024.0, 24.0),
    ] {
        let style = resolve(&rules, &theme(), env(width, false), Interaction::default()).unwrap();
        assert_eq!(style[&Property::PaddingLeft], ResolvedValue::Px(padding));
        assert_eq!(style[&Property::Height], ResolvedValue::Px(36.0));
        assert_eq!(style[&Property::Width], ResolvedValue::Percent(100.0));
        assert_eq!(style[&Property::FontSize], ResolvedValue::Px(14.0));
        assert_eq!(style[&Property::Opacity], ResolvedValue::Scalar(0.5));
    }
    let mut environment = env(800.0, false);
    environment.root_font_px = 20.0;
    let style = resolve(&rules, &theme(), environment, Interaction::default()).unwrap();
    assert_eq!(style[&Property::PaddingTop], ResolvedValue::Px(20.0));
    for width in [f32::NAN, f32::INFINITY, -1.0] {
        assert!(resolve(&rules, &theme(), env(width, false), Interaction::default()).is_err());
    }
}

#[test]
fn condition_matching_and_cascade_do_not_depend_on_class_order() {
    let first = parse("bg-background dark:bg-card dark:hover:bg-accent").unwrap();
    let second = parse("dark:hover:bg-accent bg-background dark:bg-card").unwrap();
    for dark in [false, true] {
        for hover in [false, true] {
            let state = Interaction {
                hover,
                ..Default::default()
            };
            let a = resolve(&first, &theme(), env(800.0, dark), state).unwrap();
            let b = resolve(&second, &theme(), env(800.0, dark), state).unwrap();
            assert_eq!(a, b);
            let token = if dark && hover {
                "accent"
            } else if dark {
                "card"
            } else {
                "background"
            };
            let expected = if dark {
                theme().dark[token]
            } else {
                theme().light[token]
            };
            assert_eq!(a[&Property::Background], ResolvedValue::Color(expected));
        }
    }
    for (class, state) in [
        (
            "focus-visible:ring-2",
            Interaction {
                focus_visible: true,
                ..Default::default()
            },
        ),
        (
            "active:ring-2",
            Interaction {
                active: true,
                ..Default::default()
            },
        ),
        (
            "disabled:ring-2",
            Interaction {
                disabled: true,
                ..Default::default()
            },
        ),
    ] {
        let rules = parse(class).unwrap();
        assert!(
            resolve(&rules, &theme(), env(800.0, false), Interaction::default())
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            resolve(&rules, &theme(), env(800.0, false), state).unwrap()[&Property::RingWidth],
            ResolvedValue::Px(2.0)
        );
    }
    for invalid in [
        "hover:hover:p-2",
        "md:lg:p-2",
        "p-02",
        "p-97",
        "opacity-101",
        "p-2 p-4",
        "hover:p-2 dark:p-4",
    ] {
        assert!(parse(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn native_plan_binds_state_props_theme_recipe_and_source() {
    let ir = component(
        r#"<div className="flex gap-2"><Button variant="secondary" size="sm" disabled={props.disabled} className="disabled:opacity-50">{props.label}</Button><Button className="hover:bg-accent">Other</Button><Input value={props.label} readOnly={true}/></div>"#,
    );
    let props = BTreeMap::from([
        ("label".into(), Value::String("한글🦀".into())),
        ("disabled".into(), Value::Bool(true)),
    ]);
    let interactions = BTreeMap::from([(
        "0/1".into(),
        Interaction {
            hover: true,
            ..Default::default()
        },
    )]);
    let theme = theme();
    let context = Context {
        props: &props,
        theme: &theme,
        environment: env(900.0, true),
        interactions: &interactions,
    };
    let plan = prepare_native(&ir, &context).unwrap();
    let button = &plan.root.children[0];
    assert_eq!(button.widget, "button");
    assert_eq!(button.style[&Property::Height], ResolvedValue::Px(32.0));
    assert_eq!(
        button.style[&Property::Background],
        ResolvedValue::Color(theme.dark["secondary"])
    );
    assert_eq!(button.style[&Property::Opacity], ResolvedValue::Scalar(0.5));
    assert_eq!(button.children[0].text.as_deref(), Some("한글🦀"));
    assert_eq!(
        plan.root.children[1].style[&Property::Background],
        ResolvedValue::Color(theme.dark["accent"])
    );
    assert_eq!(
        plan.root.children[2].attributes["readOnly"],
        Value::Bool(true)
    );
    assert!(plan.source.semantic_eq(&ir));
    assert_eq!(
        button.source,
        match &ir.root.kind {
            NodeKind::Element { children, .. } => children[0].span,
            _ => unreachable!(),
        }
    );
}

#[test]
fn every_initial_component_variant_size_has_a_native_plan() {
    let theme = theme();
    let props = BTreeMap::from([
        ("label".into(), Value::String("Name".into())),
        ("disabled".into(), Value::Bool(false)),
    ]);
    let interactions = BTreeMap::new();
    let context = Context {
        props: &props,
        theme: &theme,
        environment: env(900.0, false),
        interactions: &interactions,
    };
    for tag in ["Button", "Badge"] {
        let variants = if tag == "Button" {
            vec![
                "default",
                "secondary",
                "destructive",
                "outline",
                "ghost",
                "link",
            ]
        } else {
            vec!["default", "secondary", "destructive", "outline"]
        };
        for variant in variants {
            let sizes = if tag == "Button" {
                vec!["default", "sm", "lg", "icon"]
            } else {
                vec![""]
            };
            for size in sizes {
                let size = if size.is_empty() {
                    String::new()
                } else {
                    format!(" size=\"{size}\" aria-label=\"Name\"")
                };
                let ir = component(&format!(
                    "<{tag} variant=\"{variant}\"{size}>{{props.label}}</{tag}>"
                ));
                prepare_native(&ir, &context).unwrap();
            }
        }
    }
    for body in [
        "<Input/>",
        "<Label>Name</Label>",
        "<Separator orientation=\"vertical\"/>",
        "<Card><CardHeader><CardTitle>Name</CardTitle><CardDescription>Desc</CardDescription></CardHeader><CardContent>Body</CardContent><CardFooter>End</CardFooter></Card>",
    ] {
        prepare_native(&component(body), &context).unwrap();
    }
}

#[test]
fn invalid_native_components_and_imports_never_claim_support() {
    for body in [
        "<Button variant=\"unknown\"/>",
        "<Button size=\"icon\"/>",
        "<Input type=\"file\"/>",
        "<Input value=\"a\" defaultValue=\"b\"/>",
        "<Input>Invalid</Input>",
        "<Label htmlFor=\"field\"/>",
        "<Dialog/>",
        "<Button asChild={true}/>",
        "<Button disabled=\"true\"/>",
        "<div className={props.label}/>",
    ] {
        let output = meari::emit(&component(body), Language::Tsx).unwrap();
        assert!(!output.design_system.native_syntax_supported, "{body}");
    }
    for import in [
        "import './effect';",
        "import { Button as B } from '@/components/ui/button';",
        "import { Button } from 'evil';",
        "import type { Button } from '@/components/ui/button';",
        "import { Input } from '@/components/ui/input';",
    ] {
        let source = format!(
            "{import} type Props = {{}}; export function Demo(props: Props) {{ return <Button/>; }}"
        );
        assert!(meari::parse(&source, Language::Tsx).is_err(), "{source}");
    }
    let mut invalid = theme();
    invalid.dark.remove("ring");
    assert!(
        invalid
            .validate()
            .unwrap_err()
            .contains("dark theme token: ring")
    );
}
