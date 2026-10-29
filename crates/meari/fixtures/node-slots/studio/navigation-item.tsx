import { Button } from "@/components/ui/button";
type Props = { workspaceId: string; label: string; iconClass: string; selected: boolean; onNavigate: (workspaceId: string) => void; };
export function StudioNavigationItem(props: Props) {
  return <Button className="aw-navigation-item" aria-label={props.label} aria-current={props.selected} onClick={() => props.onNavigate(props.workspaceId)}><span className={props.iconClass} aria-hidden="true"/><span className="aw-navigation-label">{props.label}</span></Button>;
}
