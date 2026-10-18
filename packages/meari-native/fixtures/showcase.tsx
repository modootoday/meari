type Props = {};
export function NativeShowcase(props: Props) {
  return <main id="showcase" className="w-[760px] p-6 flex flex-col gap-4 bg-background text-foreground">
    <h1 className="text-xl font-semibold">Rust native component renderer</h1>
    <p className="text-sm text-muted-foreground">Tailwind CSS · native layout and paint · composite widget state</p>
    <Tabs id="tabs" defaultValue="overview">
      <TabsList><TabsTrigger id="tab-overview" value="overview">Overview</TabsTrigger><TabsTrigger id="tab-disabled" value="blocked" disabled={true}>Disabled</TabsTrigger><TabsTrigger id="tab-details" value="details">Details</TabsTrigger></TabsList>
      <TabsContent id="panel-overview" value="overview"><Card className="p-4"><CardTitle>Shared source, native output</CardTitle><CardDescription>Grid, arbitrary widths, semantic colors and state selectors use the compiled stylesheet.</CardDescription></Card></TabsContent>
      <TabsContent id="panel-details" value="details"><Card className="p-4"><CardTitle>Details selected</CardTitle><CardDescription>Arrow navigation skips disabled items and preserves DOM identity.</CardDescription></Card></TabsContent>
    </Tabs>
    <div className="grid grid-cols-[1fr_1fr] gap-4">
      <div className="flex flex-col gap-2"><Label htmlFor="name">Name</Label><Input id="name" defaultValue="Native input" placeholder="Name"/></div>
      <Select id="select" defaultValue="balanced"><SelectTrigger id="select-trigger"><SelectValue placeholder="Mode"/></SelectTrigger><SelectContent><SelectItem id="mode-balanced" value="balanced">Balanced</SelectItem><SelectItem id="mode-quality" value="quality">Quality</SelectItem></SelectContent></Select>
    </div>
    <Accordion defaultValue=""><AccordionItem value="about"><AccordionTrigger id="accordion-trigger">Implementation notes</AccordionTrigger><AccordionContent>The CSS engine is native Rust. Whole-catalog parity remains a separate acceptance gate.</AccordionContent></AccordionItem></Accordion>
    <Dialog id="dialog"><DialogTrigger id="dialog-trigger">Open dialog</DialogTrigger><DialogOverlay/><DialogContent id="dialog-content"><DialogTitle>Native dialog</DialogTitle><DialogDescription>Keyboard focus stays inside this modal. Escape restores the trigger.</DialogDescription><Label htmlFor="dialog-input">Dialog value</Label><Input id="dialog-input" defaultValue="Modal input"/><DialogClose id="dialog-close" className="mt-4">Close dialog</DialogClose></DialogContent></Dialog>
  </main>;
}
