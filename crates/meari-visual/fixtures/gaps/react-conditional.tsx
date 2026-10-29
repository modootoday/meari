"use client";
import { useState } from "react";
type Props = {};
export function Fixture(props: Props) {
  const [open, setOpen] = useState(true);
  return <div className="p-4">{open && <p>shown</p>}</div>;
}
