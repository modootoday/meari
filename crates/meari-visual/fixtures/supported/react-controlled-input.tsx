"use client";
import { useState } from "react";
type Props = {};
export function Fixture(props: Props) {
  const [text, setText] = useState("hello");
  return <div className="p-4"><input className="w-40 border px-2" value={text} onChange={e => setText(e.target.value)} /><p>{text}</p></div>;
}
