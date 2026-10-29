"use client";
import { useState } from "react";
type Props = {};
export function Fixture(props: Props) {
  const [items, setItems] = useState(["Apple", "Banana", "Cherry"]);
  return (
    <div className="p-4">
      <ul className="space-y-1">{items.map(item => <li key={item} className="rounded bg-slate-100 px-2">{item}</li>)}</ul>
      <button className="mt-2 rounded bg-blue-600 px-3 py-1 text-white" onClick={() => setItems(["Apple", "Banana", "Cherry", "Date"])}>Add</button>
    </div>
  );
}
