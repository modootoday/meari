import { useState } from "react";
export function StructuredState() {
  const [state, setState] = useState({ count: 0, rows: ["first", "second"], zero: -0 });
  return <button onClick={() => setState({ count: 1, rows: ["next"], zero: -0 })}>Update structured state</button>;
}
