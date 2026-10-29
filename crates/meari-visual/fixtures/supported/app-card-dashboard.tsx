import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
type Props = {};
export function Dashboard(props: Props) {
  return <div className="grid grid-cols-2 gap-4 bg-muted p-4"><Card><CardHeader><CardTitle>Revenue</CardTitle><CardDescription>This month</CardDescription></CardHeader><CardContent><p className="text-2xl font-bold">$12,400</p></CardContent></Card><Card><CardHeader><CardTitle>Users</CardTitle><CardDescription>Active</CardDescription></CardHeader><CardContent><Badge>+12%</Badge></CardContent></Card></div>;
}
