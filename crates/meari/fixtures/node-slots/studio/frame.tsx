import type { ReactNode } from "react";
import { WindowChrome } from "../components/window-chrome";
type Props = { productName: string; contextLabel: string; navigation: ReactNode; workspace: ReactNode; inspector: ReactNode; queue: ReactNode; };
export function StudioFrame(props: Props) {
  return <main className="aw-studio-shell"><WindowChrome productName={props.productName} contextLabel={props.contextLabel}/><div className="aw-studio-layout">{props.navigation}<div className="aw-workspace-slot">{props.workspace}</div>{props.inspector}</div>{props.queue}</main>;
}
