"use client";
import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
type Props = {};
export function Counter(props: Props) {
  const [count, setCount] = useState(0);
  const [label, setLabel] = useState("");
  const [disabled, setDisabled] = useState(false);
  return <main>
    <p id="status">Count {count}<span id="units"> units</span>{label}</p>
    <Button id="increment" disabled={disabled} onClick={() => setCount(previous => previous + 1)}>Increment</Button>
    <Button id="rename" onClick={() => setLabel(" ready🦀")}>Rename</Button>
    <Button id="toggle" onClick={() => setDisabled(previous => !previous)}>Toggle</Button>
    <Input id="name" aria-label="Name" defaultValue={label} />
  </main>;
}
