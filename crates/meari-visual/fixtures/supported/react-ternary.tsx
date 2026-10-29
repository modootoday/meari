"use client";
import { useState } from "react";
type Props = {};
export function Fixture(props: Props) {
  const [on, setOn] = useState(true);
  return <div className="p-4">{on ? <p className="text-green-700">On</p> : <p className="text-red-700">Off</p>}</div>;
}
