import {useId} from 'react';
type Props={onSave:()=>void};
export function Editor(props:Props){const field=useId();return <article aria-label="editor"><label htmlFor={field}>Editor</label><input id={field} aria-label="owned"/><button onClick={()=>props.onSave()}>save</button></article>}
