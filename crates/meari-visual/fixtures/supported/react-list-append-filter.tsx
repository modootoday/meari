"use client";
import { useState } from "react";
type Props = {};
export function Fixture(props: Props) {
  const [todos, setTodos] = useState(["Buy milk", "Walk dog", "Write code"]);
  return (
    <div className="space-y-2 p-4">
      <div className="flex gap-2">
        <button className="rounded bg-blue-600 px-3 py-1 text-white" onClick={() => setTodos([...todos, "Read book"])}>Add</button>
        <button className="rounded bg-red-600 px-3 py-1 text-white" onClick={() => setTodos(todos.filter(todo => todo !== "Walk dog"))}>Remove</button>
      </div>
      <ul className="divide-y divide-zinc-200 rounded border border-zinc-200">{todos.map(todo => <li key={todo} className="px-3 py-1">{todo}</li>)}</ul>
    </div>
  );
}
