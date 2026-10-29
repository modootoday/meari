import {Transport} from './transport';
type Props={value:string;preview:string;positionLabel:string;durationLabel:string;disabled:boolean;onDraft:(value:string)=>void;onCommit:(value:string)=>void};
export function ReadRoot(props:Props){return <section aria-label="read-root"><Transport value={props.value} preview={props.preview} positionLabel={props.positionLabel} durationLabel={props.durationLabel} disabled={props.disabled} onDraft={props.onDraft} onCommit={props.onCommit}/></section>}
