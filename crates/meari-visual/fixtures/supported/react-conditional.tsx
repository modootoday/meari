"use client";
import { useState } from "react";
type Props = {};
export function Fixture(props: Props) {
  const [open, setOpen] = useState(true);
  return <div className="flex flex-col gap-2 p-4">{open && <p className="flex">shown</p>}{!open && <p>not shown</p>}<p>after</p></div>;
}
