"use client";
import { Input } from "@/components/ui/input";
type Props = {};
export function Fixture(props: Props) {
  return <div className="max-w-sm p-6"><Input id="email" type="email" placeholder="you@example.com" /></div>;
}
