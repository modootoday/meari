import { useId, useState } from "react";
type Props = {};
export function MixedHooks(props: Props) {
  const field = useId();
  const [count, setCount] = useState(0);
  const help = useId();
  const [enabled, setEnabled] = useState(true);
  return <div><label htmlFor={field}>Name</label><input id={field} aria-describedby={help}/><p id={help}>{count}</p><button disabled={enabled} onClick={() => setCount(previous => previous + 1)}>Add</button><button onClick={() => setEnabled(previous => !previous)}>Toggle</button></div>;
}
