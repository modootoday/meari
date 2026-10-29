use crate::{
    application::markup::Element,
    domain::{
        Viewport,
        widgets::{Action, Session},
    },
    infrastructure::engine::NativeRenderer,
};
use meari::domain::Component;
use std::collections::BTreeMap;

/// One persistent native DOM. Widget updates patch attributes/text, preserving input/scroll nodes.
pub struct InteractiveDocument {
    pub renderer: NativeRenderer,
    pub session: Session,
    pub source: Component,
    pub viewport: Viewport,
    pub last_error: Option<String>,
    pub input_values: BTreeMap<String, String>,
    pub(super) range_inputs: super::range_input::RangeInputs,
    pub activation_requests: Vec<String>,
    pub file_values: BTreeMap<String, Vec<crate::application::file_picker::SelectedFile>>,
    pub file_requests: Vec<String>,
    pub checked_values: BTreeMap<String, bool>,
    pub(super) disclosure_values: BTreeMap<String, bool>,
    pub(super) elements: Vec<Element>,
    pub(super) responsive: Option<meari::application::responsive::ResponsiveSession>,
    pub(super) media_bindings:
        Option<meari::application::component_state::media::MediaStateBindings>,
    pub media_changes: Vec<meari::application::media::MediaChange>,
    pub(super) state_program: Option<meari::application::state_program::StateProgram>,
    pub(super) module_program: Option<meari::application::source_modules::ModuleProgram>,
    pub(super) hook_actions: Vec<meari::application::hook_document::BoundAction>,
    pub(super) state_events: BTreeMap<String, meari::domain::state_program::StateEvent>,
    pub(super) state_inputs: BTreeMap<String, meari::domain::input_event::InputStateBinding>,
    pub(super) source_input_poisoned: bool,
    pub(super) callbacks: Option<super::source_callbacks::CallbackSession>,
    pub(super) source_schema: meari::domain::source_schema::SourceSchema,
    pub(super) application: Option<meari::application::app_bundle::ApplicationDefinition>,
    pub(super) application_generation: u64,
    pub(super) application_definition_revision: u64,
}

struct QueuedHandler<'a> {
    session: &'a Session,
    actions: &'a mut Vec<Action>,
    inputs: &'a mut BTreeMap<String, String>,
    source_inputs: &'a BTreeMap<String, meari::domain::input_event::InputStateBinding>,
    input_events: &'a mut Vec<(String, meari::domain::input_event::InputPayload)>,
    callback_inputs: &'a std::collections::BTreeSet<String>,
}

mod construction;
mod requests;
pub(super) use self::requests::*;
mod ui_events;
