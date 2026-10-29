import { useState } from "react";
export function StructuredState() {
  const [state, setState] = useState({ count: 0, rows: [1, 2] });
  return <button onClick={() => setState(current => ({ ...current, count: current.count + 1, rows: state.rows }))}>Update</button>;
}
