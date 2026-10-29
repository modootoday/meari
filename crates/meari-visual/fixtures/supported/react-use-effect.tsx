"use client";
import { useEffect, useState } from "react";
type Props = {};
export function Fixture(props: Props) {
  const [n, setN] = useState(0);
  useEffect(() => { setN(1); }, []);
  return <p>{n}</p>;
}
