import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
type Props = {};
export function ButtonInput(props: Props) {
  return <div className="form"><Input id="name" aria-label="Name" defaultValue="Kino" /><Button id="go">Save</Button></div>;
}
