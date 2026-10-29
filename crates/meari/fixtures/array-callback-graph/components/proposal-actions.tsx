import { Button } from "@/components/ui/button";

type Props = {
  canSave: boolean; canAccept: boolean; busy: boolean; refreshRequired: boolean;
  resultLabel: string; onSave: () => void; onAccept: () => void; onRefresh: () => void;
};

export function ProposalActions(props: Props) {
  return <div className="aw-inspector-footer">
    <Button className="aw-button" disabled={!props.canSave || props.busy || props.refreshRequired} onClick={() => props.onSave()}>수정본 저장</Button>
    <Button className="aw-button aw-button-primary" disabled={!props.canAccept || props.busy || props.refreshRequired} onClick={() => props.onAccept()}>명시적 채택</Button>
    <Button className="aw-button" hidden={!props.refreshRequired} disabled={props.busy} onClick={() => props.onRefresh()}>저장 결과 다시 확인</Button>
    <p role="status">{props.resultLabel}</p>
  </div>;
}
