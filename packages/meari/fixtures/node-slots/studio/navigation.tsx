import { StudioNavigationItem } from "./navigation-item";
type Props = { selectedId: string; onNavigate: (workspaceId: string) => void; };
export function StudioNavigation(props: Props) {
  return <nav className="aw-studio-navigation" aria-label="제작 작업 탐색">
    <StudioNavigationItem workspaceId="VIEW" label="시청으로 돌아가기" iconClass="aw-icon aw-icon-back" selected={false} onNavigate={props.onNavigate}/>
    <div className="aw-navigation-tools">
      <StudioNavigationItem workspaceId="SEG" label="구간 감지" iconClass="aw-icon aw-icon-segments" selected={props.selectedId === "SEG"} onNavigate={props.onNavigate}/>
      <StudioNavigationItem workspaceId="SUB" label="자막" iconClass="aw-icon aw-icon-subtitles" selected={props.selectedId === "SUB"} onNavigate={props.onNavigate}/>
      <StudioNavigationItem workspaceId="OBJ" label="피사체" iconClass="aw-icon aw-icon-subject" selected={props.selectedId === "OBJ"} onNavigate={props.onNavigate}/>
      <StudioNavigationItem workspaceId="CLIP" label="숏폼" iconClass="aw-icon aw-icon-clip" selected={props.selectedId === "CLIP"} onNavigate={props.onNavigate}/>
      <StudioNavigationItem workspaceId="FRAME" label="스냅샷" iconClass="aw-icon aw-icon-image" selected={props.selectedId === "FRAME"} onNavigate={props.onNavigate}/>
      <StudioNavigationItem workspaceId="CARD" label="카드뉴스" iconClass="aw-icon aw-icon-cards" selected={props.selectedId === "CARD"} onNavigate={props.onNavigate}/>
      <StudioNavigationItem workspaceId="A2D" label="2D 에셋" iconClass="aw-icon aw-icon-layers" selected={props.selectedId === "A2D"} onNavigate={props.onNavigate}/>
      <StudioNavigationItem workspaceId="PROJ" label="3D 투영" iconClass="aw-icon aw-icon-cube" selected={props.selectedId === "PROJ"} onNavigate={props.onNavigate}/>
      <StudioNavigationItem workspaceId="SAVE" label="영상 저장" iconClass="aw-icon aw-icon-export" selected={props.selectedId === "SAVE"} onNavigate={props.onNavigate}/>
      <StudioNavigationItem workspaceId="MODEL" label="모델 관리" iconClass="aw-icon aw-icon-model" selected={props.selectedId === "MODEL"} onNavigate={props.onNavigate}/>
    </div>
    <div className="aw-navigation-bottom"><StudioNavigationItem workspaceId="JOB" label="작업 대기열" iconClass="aw-icon aw-icon-episodes" selected={props.selectedId === "JOB"} onNavigate={props.onNavigate}/><StudioNavigationItem workspaceId="OUT" label="내보내기" iconClass="aw-icon aw-icon-export" selected={props.selectedId === "OUT"} onNavigate={props.onNavigate}/></div>
  </nav>;
}
