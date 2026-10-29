type Props = {};
export function Fixture(props: Props) {
  return <div className="flex gap-3 bg-background p-6"><div className="rounded-md bg-primary px-4 py-2 text-primary-foreground">Primary</div><div className="rounded-md bg-secondary px-4 py-2 text-secondary-foreground">Secondary</div><div className="rounded-md border border-border bg-muted px-4 py-2 text-muted-foreground">Muted</div></div>;
}
