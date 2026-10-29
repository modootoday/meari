import { useState } from "react";
type Props = { caption: string; disabled: boolean };
export function Counter(props: Props) {
  const [state, setState] = useState({ count: 0, rows: [1, 2] });
  return <section><button disabled={props.disabled} onClick={() => setState(previous => ({ ...previous, count: previous.count + 1 }))}>{props.caption}</button><input defaultValue="keep"/></section>;
}
