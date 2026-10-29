"use client";
import { useState } from "react";
import { Button } from "@/components/ui/button";
type Props = {};
export function CounterApp(props: Props) {
  const [count, setCount] = useState(5);
  return <main className="flex items-center gap-4 p-6"><Button id="dec" variant="outline" onClick={() => setCount(previous => previous - 1)}>-</Button><span id="count" className="w-12 text-center text-2xl font-bold">{count}</span><Button id="inc" onClick={() => setCount(previous => previous + 1)}>+</Button></main>;
}
