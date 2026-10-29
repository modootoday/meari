"use client";
import { useReducer } from "react";
type Props = {};
function reducer(state: number, action: string) { return action === "inc" ? state + 1 : state; }
export function Fixture(props: Props) {
  const [n, dispatch] = useReducer(reducer, 0);
  return <div className="p-4"><button onClick={() => dispatch("inc")}>+</button><p>{n}</p></div>;
}
