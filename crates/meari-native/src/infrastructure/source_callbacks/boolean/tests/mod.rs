//! Controlled Boolean tests use the public module factory and its scoped preflight.
use super::*;
mod profiles;
use meari::application::plugins::*;
struct Adapter(PluginManifest);

const DIALOG: &str = "type Props={checked:boolean;disabled:boolean;change:(value:boolean)=>void};export function Panel(props:Props){return <div><Button>background</Button><Dialog open={props.checked} onOpenChange={props.change}><DialogTrigger disabled={props.disabled}>open</DialogTrigger><DialogContent forceMount hidden={!props.checked} aria-label=\"inspector\"><Input defaultValue=\"draft\"/><DialogClose>close</DialogClose></DialogContent><DialogOverlay/></Dialog></div>;}";

const ORIGINAL_DOCK_ENTRY: &str = "import {InspectorDock} from './dock';type Props={checked:boolean;disabled:boolean;change:(value:boolean)=>void};export function Panel(props:Props){return <InspectorDock narrow={!props.disabled} open={props.checked} title=\"Inspector\" onOpenChange={props.change}><Input defaultValue=\"draft\" aria-label=\"slot editor\"/></InspectorDock>;}";

mod harness;
use self::harness::*;
#[cfg(test)]
mod commit_intent;
#[cfg(test)]
mod focus_recovery;
