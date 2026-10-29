//! Scoped derived CSS lives outside the declared frame; input attributes/leases stay intact.
use super::super::{
    WebDocument,
    frame::{Frame, error},
    listener::Listener,
};
use crate::domain::range_parts::{ATTRIBUTE, RangeParts, fraction_from_actual};
use std::{collections::BTreeSet, fmt::Write, rc::Rc};
use wasm_bindgen::JsCast;
use web_sys::{Element, HtmlInputElement};
const STYLE_SELECTOR: &str = ":scope > style[data-meari-range-parts-style]";
fn id_selector(id: &str) -> String {
    let mut output = String::from("#");
    for character in id.chars() {
        write!(output, "\\{:x} ", u32::from(character)).expect("String writing");
    }
    output
}
pub(in super::super) fn sync_frame(frame: &Frame, shell: &Element) -> Result<(), String> {
    let mut values = Vec::new();
    let mut ids = BTreeSet::new();
    for node in frame.nodes.values() {
        let Some(input) = node.dyn_ref::<HtmlInputElement>() else {
            continue;
        };
        let Some(profile) = input.get_attribute(ATTRIBUTE) else {
            continue;
        };
        if input.type_() != "range" {
            return Err("range parts require the original range input".into());
        }
        RangeParts::parse(Some(&profile))?;
        let id = input.id();
        if id.is_empty()
            || id.len() > 256
            || id.contains('\0')
            || !ids.insert(id.clone())
            || ids.len() > 1024
        {
            return Err("range parts require unique bounded original ids".into());
        }
        if !shell.contains(Some(input.as_ref())) {
            return Err("range parts input is detached".into());
        }
        let min = input
            .min()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .unwrap_or(0.0);
        let max = input
            .max()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .unwrap_or(100.0);
        values.push((id, fraction_from_actual(input.value_as_number(), min, max)?));
    }
    let style = shell.query_selector(STYLE_SELECTOR).map_err(error)?;
    if values.is_empty() && style.is_none() {
        return Ok(());
    }
    let scope = shell
        .get_attribute("data-meari-scope")
        .ok_or("range parts require original shell scope")?;
    let mut css = crate::application::range_parts::web_stylesheet(&scope)?;
    for (id, fraction) in values {
        writeln!(
            css,
            "[data-meari-scope='{scope}'] input{}[data-meari-range-parts]{{--meari-range-position:{}%;}}",
            id_selector(&id),
            fraction * 100.0
        )
        .expect("String writing");
    }
    let style = match style {
        Some(style) => style,
        None => {
            let style = shell
                .owner_document()
                .ok_or("range parts document missing")?
                .create_element("style")
                .map_err(error)?;
            style
                .set_attribute("data-meari-range-parts-style", "")
                .map_err(error)?;
            shell.append_child(&style).map_err(error)?;
            style
        }
    };
    style.set_text_content(Some(&css));
    Ok(())
}
impl WebDocument {
    pub(in super::super) fn install_range_parts_dispatcher(&mut self) -> Result<(), String> {
        let weak = Rc::downgrade(&self.inner);
        self.listeners.push(Listener::capture(
            self.inner.borrow().shell.clone().unchecked_into(),
            "input",
            move |event| {
                let Some(owner) = weak.upgrade() else { return };
                let mut inner = owner.borrow_mut();
                if inner.closed || inner.poisoned {
                    return;
                }
                let Some(target) = event
                    .target()
                    .and_then(|target| target.dyn_into::<HtmlInputElement>().ok())
                    .filter(|input| input.has_attribute(ATTRIBUTE))
                else {
                    return;
                };
                if !inner
                    .frame
                    .nodes
                    .values()
                    .any(|node| node.is_same_node(Some(target.as_ref())))
                {
                    return;
                }
                let result = inner
                    .frame
                    .preflight(&inner.frame.plan, &inner.shell)
                    .and_then(|()| sync_frame(&inner.frame, &inner.shell));
                if let Err(error) = result {
                    inner.last_error = Some(error);
                    inner.poisoned = true;
                }
            },
        )?);
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::super::id_selector;
    #[wasm_bindgen_test::wasm_bindgen_test]
    fn css_identifier_encoding_never_interpolates_source_css() {
        assert_eq!(id_selector("x']{}"), "#\\78 \\27 \\5d \\7b \\7d ");
    }
}
