"use client";
import { useReducer } from "react";
type Props = {};
function reducer(state: number, action: { type: string }) {
  switch (action.type) {
    case "incremented": {
      return state + 1;
    }
    case "decremented": {
      return state - 1;
    }
  }
  throw Error("Unknown action: " + action.type);
}
export function Fixture(props: Props) {
  const [age, dispatch] = useReducer(reducer, 42);
  return (
    <div className="flex items-center gap-3 p-4">
      <button className="rounded bg-zinc-200 px-3 py-1" onClick={() => dispatch({ type: "decremented" })}>-</button>
      <p className="text-lg font-semibold">Age {age}</p>
      <button className="rounded bg-blue-600 px-3 py-1 text-white" onClick={() => dispatch({ type: "incremented" })}>+</button>
    </div>
  );
}
