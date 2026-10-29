import type { ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogTrigger, DialogContent, DialogTitle, DialogClose } from "@/components/ui/dialog";
type Props = { narrow: boolean; open: boolean; title: string; children: ReactNode; onOpenChange: (open: boolean) => void; };
export function InspectorDock(props: Props) {
  return <Dialog open={!props.narrow || props.open} modal={props.narrow} onOpenChange={props.onOpenChange}>
    <div className="aw-inspector-trigger" hidden={!props.narrow}><DialogTrigger asChild={true}><Button className="aw-button" aria-label="검사기 설정 열기">설정</Button></DialogTrigger></div>
    <DialogContent className="aw-inspector-dock" aria-label={props.title} forceMount={true} hidden={props.narrow && !props.open}>
      <div className="aw-inspector-dock-header" hidden={!props.narrow}><DialogTitle>{props.title}</DialogTitle><DialogClose asChild={true}><Button className="aw-icon-button" aria-label="검사기 설정 닫기"><span className="aw-icon aw-icon-close" aria-hidden="true"/></Button></DialogClose></div>
      {props.children}
    </DialogContent>
  </Dialog>;
}
