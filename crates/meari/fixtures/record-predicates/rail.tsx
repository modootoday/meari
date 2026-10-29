type Props={items:{key:string;editable:boolean}[];selectedKey:string;onSelect:(key:string)=>void};
export function Rail(props:Props){return <div>{props.items.map(item=><button key={item.key} disabled={!item.editable} aria-disabled={!item.editable} onClick={()=>props.onSelect(props.selectedKey)}>{item.key}</button>)}</div>}
