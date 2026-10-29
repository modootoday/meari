import { useState } from "react";
export function StructuredArray() {
  const [items, setItems] = useState([1, 2]);
  return <button onClick={() => setItems([3])}>Update structured array</button>;
}
