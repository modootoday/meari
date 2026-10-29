import type {ReactNode} from 'react';
type Props={left:ReactNode;right:ReactNode};
export function Mixed(props:Props){return <main><span>before</span>{props.left}<input aria-label="fixed" defaultValue="fixed"/>{props.right}<span>after</span></main>}
