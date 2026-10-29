//! Native range default action. Existing source/host input paths own callbacks.
mod value;
use blitz_dom::NodeId;
use blitz_html::HtmlDocument;
use blitz_traits::events::{BlitzPointerId, MouseEventButton, MouseEventButtons, UiEvent};
use meari::domain::range_interaction::{RangeGesture, RangeInteraction, RangeValueCommit};
use value::RangeValue;
#[derive(Clone, PartialEq)]
struct Owner {
    document: usize,
    epoch: u64,
    definition: u64,
    node: NodeId,
    path: String,
    callback: Option<meari::application::host_callbacks::CallbackEventOwner>,
}
type Owners =
    std::collections::BTreeMap<String, meari::application::host_callbacks::CallbackEventOwner>;
fn pointer_id(pointer: BlitzPointerId) -> u128 {
    match pointer {
        BlitzPointerId::Mouse => 0,
        BlitzPointerId::Pen => 1,
        BlitzPointerId::Finger(id) => u128::from(id) + 2,
    }
}
fn valid(doc: &HtmlDocument, owner: &Owner, epoch: u64, definition: u64, owners: &Owners) -> bool {
    doc.id() == owner.document
        && epoch == owner.epoch
        && definition == owner.definition
        && doc.get_focussed_node_id() == Some(owner.node)
        && is_range(doc, owner.node)
        && !disabled(doc, owner.node)
        && attr(doc, owner.node, "data-bridge-path") == Some(owner.path.as_str())
        && owners.get(&owner.path) == owner.callback.as_ref()
        && owner
            .callback
            .as_ref()
            .is_none_or(|callback| callback.is_active())
}
const CSS: &str = r#"
input[type=range]{display:inline-block;width:160px;height:24px;box-sizing:border-box;
 padding:0;border:0;background-color:transparent;
 background-image:radial-gradient(circle,currentColor 0 48%,transparent 52%),linear-gradient(currentColor,currentColor);
 background-repeat:no-repeat;background-size:16px 16px,calc(100% - 16px) 4px;
 background-position:var(--meari-range-position,50%) center,center;cursor:pointer}
input[type=range]:disabled{opacity:.5;cursor:default}
input[type=range]:focus-visible{outline:2px solid currentColor;outline-offset:2px}
"#;
pub(super) fn configure(mut doc: HtmlDocument) -> HtmlDocument {
    let css = format!(
        "{CSS}\n{}",
        meari::application::range_parts::native_stylesheet()
    );
    doc.add_user_agent_stylesheet(blitz_dom::UserAgentStylesheet::parse(&css));
    doc
}
fn parts(doc: &HtmlDocument, id: NodeId) -> Result<meari::domain::range_parts::RangeParts, String> {
    let profile = attr(doc, id, meari::domain::range_parts::ATTRIBUTE);
    if profile.is_some()
        && attr(doc, id, "id").is_none_or(|id| id.is_empty() || id.len() > 256 || id.contains('\0'))
    {
        return Err("range parts require an original bounded input id".into());
    }
    meari::domain::range_parts::RangeParts::parse(profile)
}
fn attr<'a>(doc: &'a HtmlDocument, id: NodeId, name: &str) -> Option<&'a str> {
    doc.get_node(id)?.attr(blitz_dom::LocalName::from(name))
}
pub(super) fn is_range(doc: &HtmlDocument, id: NodeId) -> bool {
    doc.get_node(id)
        .and_then(|n| n.element_data())
        .is_some_and(|e| AsRef::<str>::as_ref(&e.name.local) == "input")
        && attr(doc, id, "type") == Some("range")
}
fn read(doc: &HtmlDocument, id: NodeId) -> Result<RangeValue, String> {
    RangeValue::read(
        attr(doc, id, "min"),
        attr(doc, id, "max"),
        attr(doc, id, "step"),
        attr(doc, id, "value"),
    )
}
fn disabled(doc: &HtmlDocument, id: NodeId) -> bool {
    let mut next = Some(id);
    while let Some(id) = next {
        let Some(node) = doc.get_node(id) else {
            return true;
        };
        if attr(doc, id, "disabled").is_some() {
            return true;
        }
        next = node.parent;
    }
    false
}
fn set_attr(doc: &mut HtmlDocument, id: NodeId, key: &str, value: &str) {
    if attr(doc, id, key) != Some(value) {
        doc.mutate()
            .set_attribute(id, super::document::name(key), value);
    }
}
fn paint(doc: &mut HtmlDocument, id: NodeId, range: RangeValue) {
    doc.set_style_property(
        id,
        "--meari-range-position",
        &format!("{}%", range.fraction() * 100.0),
    );
    set_attr(doc, id, "aria-valuemin", &range.min.to_string());
    set_attr(doc, id, "aria-valuemax", &range.max.to_string());
    set_attr(doc, id, "aria-valuenow", &range.value.to_string());
}
pub(super) fn refresh(doc: &mut HtmlDocument) -> Result<(), String> {
    let mut queue = vec![doc.root_element().id];
    let mut ids = Vec::new();
    while let Some(id) = queue.pop() {
        let Some(node) = doc.get_node(id) else {
            continue;
        };
        queue.extend(node.children.iter().copied());
        if is_range(doc, id) {
            ids.push(id);
        }
    }
    for id in ids {
        parts(doc, id)?;
        let range = read(doc, id)?;
        paint(doc, id, range);
    }
    Ok(())
}
struct Drag {
    document: usize,
    epoch: u64,
    node: NodeId,
    pointer: BlitzPointerId,
    path: String,
}
#[derive(Default)]
pub(super) struct RangeInputs {
    drag: Option<Drag>,
    interaction: RangeInteraction<Owner>,
    pending: Option<RangeValueCommit<Owner>>,
}
pub(super) struct Outcome {
    pub path: String,
    pub value: Option<String>,
    pub release: Option<RangeGesture>,
}
impl RangeInputs {
    pub fn is_active(
        &self,
        doc: &HtmlDocument,
        epoch: u64,
        definition: u64,
        owners: &Owners,
    ) -> bool {
        self.interaction
            .owner()
            .is_some_and(|owner| valid(doc, owner, epoch, definition, owners))
    }
    pub fn cancel(&mut self) {
        self.drag = None;
        self.interaction.cancel();
        self.pending = None;
    }
    pub fn pending(
        &mut self,
        doc: &HtmlDocument,
        epoch: u64,
        definition: u64,
        owners: &Owners,
    ) -> Option<(String, String)> {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| !valid(doc, &pending.owner, epoch, definition, owners))
        {
            self.cancel();
        }
        self.pending
            .as_ref()
            .map(|pending| (pending.owner.path.clone(), pending.value.clone()))
    }
    pub fn acknowledge(&mut self) {
        self.pending = None;
    }
    pub fn finish(
        &mut self,
        doc: &HtmlDocument,
        epoch: u64,
        definition: u64,
        owners: &Owners,
        release: Option<&RangeGesture>,
    ) -> Result<(), String> {
        let Some(release) = release else {
            return Ok(());
        };
        let Some(owner) = self.interaction.owner().cloned() else {
            return Ok(());
        };
        if !valid(doc, &owner, epoch, definition, owners) {
            self.cancel();
            return Ok(());
        }
        self.pending =
            self.interaction
                .finish(&owner, release, &read(doc, owner.node)?.value.to_string())?;
        Ok(())
    }
    /// Some consumes the event even if the value did not change.
    pub fn handle(
        &mut self,
        doc: &mut HtmlDocument,
        epoch: u64,
        definition: u64,
        owners: &Owners,
        event: &UiEvent,
    ) -> Result<Option<Outcome>, String> {
        if self
            .interaction
            .owner()
            .is_some_and(|owner| !valid(doc, owner, epoch, definition, owners))
        {
            self.cancel();
        }
        if self.drag.as_ref().is_some_and(|d| {
            d.document != doc.id()
                || d.epoch != epoch
                || doc.get_focussed_node_id() != Some(d.node)
                || !is_range(doc, d.node)
                || disabled(doc, d.node)
                || attr(doc, d.node, "data-bridge-path") != Some(d.path.as_str())
        }) {
            self.cancel();
        }
        match event {
            UiEvent::PointerCancel(_) | UiEvent::PointerLeave(_) => {
                self.cancel();
                return Ok(None);
            }
            UiEvent::KeyDown(key) if ["Tab", "Escape"].contains(&key.key.to_string().as_str()) => {
                self.cancel();
                return Ok(None);
            }
            _ => {}
        }
        if let UiEvent::KeyUp(key) = event {
            if key.is_composing {
                return Ok(None);
            }
            if let Some(owner) = self.interaction.owner() {
                return Ok(Some(Outcome {
                    path: owner.path.clone(),
                    value: None,
                    release: Some(RangeGesture::Key(key.key.to_string())),
                }));
            }
        }
        let (id, path, candidate, finish) = match event {
            UiEvent::PointerDown(p) if p.is_primary && p.button == MouseEventButton::Main => {
                let Some(id) = doc.element_from_point(p.coords.client_x, p.coords.client_y) else {
                    return Ok(None);
                };
                if !is_range(doc, id) {
                    self.cancel();
                    return Ok(None);
                }
                let path = attr(doc, id, "data-bridge-path")
                    .ok_or("range has no interactive owner path")?
                    .to_owned();
                if disabled(doc, id) {
                    self.cancel();
                    return Ok(Some(Outcome {
                        path,
                        value: None,
                        release: None,
                    }));
                }
                if doc.get_focussed_node_id() != Some(id) && !doc.set_focus_to(id) {
                    return Err("range focus request failed".into());
                }
                let candidate = pointer_value(doc, id, p.coords.client_x)?;
                self.interaction.begin(
                    Owner {
                        document: doc.id(),
                        epoch,
                        definition,
                        node: id,
                        callback: owners.get(&path).cloned(),
                        path: path.clone(),
                    },
                    RangeGesture::Pointer(pointer_id(p.id)),
                    &read(doc, id)?.value.to_string(),
                )?;
                self.drag = Some(Drag {
                    document: doc.id(),
                    epoch,
                    node: id,
                    pointer: p.id,
                    path: path.clone(),
                });
                (id, path, candidate, false)
            }
            UiEvent::PointerMove(p) | UiEvent::PointerUp(p) => {
                let Some(d) = &self.drag else {
                    // A repeated release on a range has no gesture to finish. Blitz's
                    // generic unsupported-input click action would clear its focus.
                    if matches!(event, UiEvent::PointerUp(_))
                        && let Some(id) = doc
                            .element_from_point(p.coords.client_x, p.coords.client_y)
                            .filter(|id| is_range(doc, *id))
                        && let Some(path) = attr(doc, id, "data-bridge-path")
                    {
                        return Ok(Some(Outcome {
                            path: path.to_owned(),
                            value: None,
                            release: None,
                        }));
                    }
                    return Ok(None);
                };
                if p.id != d.pointer {
                    return Ok(None);
                }
                if matches!(event, UiEvent::PointerMove(_))
                    && !p.buttons.contains(MouseEventButtons::Primary)
                {
                    self.cancel();
                    return Ok(None);
                }
                (
                    d.node,
                    d.path.clone(),
                    pointer_value(doc, d.node, p.coords.client_x)?,
                    matches!(event, UiEvent::PointerUp(_)),
                )
            }
            UiEvent::KeyDown(key) if !key.is_composing => {
                let Some(id) = doc.get_focussed_node_id().filter(|id| is_range(doc, *id)) else {
                    return Ok(None);
                };
                let path = attr(doc, id, "data-bridge-path")
                    .ok_or("range path missing")?
                    .to_owned();
                if disabled(doc, id) {
                    self.cancel();
                    return Ok(Some(Outcome {
                        path,
                        value: None,
                        release: None,
                    }));
                }
                let Some(value) = read(doc, id)?.key(&key.key.to_string())? else {
                    return Ok(None);
                };
                self.drag = None;
                self.interaction.begin(
                    Owner {
                        document: doc.id(),
                        epoch,
                        definition,
                        node: id,
                        callback: owners.get(&path).cloned(),
                        path: path.clone(),
                    },
                    RangeGesture::Key(key.key.to_string()),
                    &read(doc, id)?.value.to_string(),
                )?;
                (id, path, value, false)
            }
            _ => return Ok(None),
        };
        let mut range = read(doc, id)?;
        let changed = candidate != range.value;
        range.value = candidate;
        if finish {
            self.drag = None;
        }
        if changed {
            set_attr(doc, id, "value", &candidate.to_string());
            paint(doc, id, range);
        }
        Ok(Some(Outcome {
            path,
            value: changed.then(|| candidate.to_string()),
            release: if finish {
                if let UiEvent::PointerUp(pointer) = event {
                    Some(RangeGesture::Pointer(pointer_id(pointer.id)))
                } else {
                    None
                }
            } else {
                None
            },
        }))
    }
}
fn pointer_value(doc: &HtmlDocument, id: NodeId, x: f32) -> Result<f64, String> {
    let rect = doc
        .get_client_bounding_rect(id)
        .ok_or("range has no layout")?;
    let fraction = parts(doc, id)?
        .geometry()
        .pointer_fraction(rect.width, f64::from(x) - rect.x)?;
    read(doc, id)?.pointer(fraction)
}
