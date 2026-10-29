"use client";
import { useState } from "react";
type Props = {};
export function Counter(props: Props) {
  const [count, setCount] = useState(3);
  const [open, setOpen] = useState(true);
  return <main className="wrap"><div className="bar" hidden={!open}></div><p id="status">Count {count}</p></main>;
}
