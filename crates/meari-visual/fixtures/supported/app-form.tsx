"use client";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
type Props = {};
export function FormApp(props: Props) {
  return <div className="flex max-w-sm flex-col gap-3 p-6"><Label htmlFor="email">Email</Label><Input id="email" type="email" defaultValue="you@example.com" /><Label htmlFor="name">Name</Label><Input id="name" defaultValue="Meari" /><Button id="submit" className="mt-2">Sign up</Button></div>;
}
