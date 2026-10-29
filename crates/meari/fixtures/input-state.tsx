"use client";
import { useState } from "react";
import { Input } from "@/components/ui/input";
type Props = {};
export function InputDemo(props: Props) {
  const [text, setText] = useState("");
  const [checked, setChecked] = useState(false);
  return <main>
    <Input id="editor" value={text} onChange={event => setText(event.currentTarget.value)} />
    <input id="check" type="checkbox" checked={checked} onChange={event => setChecked(event.target.checked)} />
    <p>{text}</p>
  </main>;
}
