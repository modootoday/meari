import { useId } from "react";
import { Button } from "@/components/ui/button";
type Props = {
  proposalId: string; sourceSha256: string; documentRevision: string;
  workingText: string; errorText: string; inputDisabled: boolean; saveDisabled: boolean; acceptDisabled: boolean; refreshDisabled: boolean;
  onDraftChange: (value: string) => void; onSave: () => void; onAccept: () => void; onRefresh: () => void;
};
export function ProposalReviewIntent(props: Props) {
  const draftId = useId();
  return <section aria-label="제안 검토 입력 연결">
    <dl><dt>제안</dt><dd>{props.proposalId}</dd><dt>원문 SHA</dt><dd>{props.sourceSha256}</dd><dt>문서 버전</dt><dd>{props.documentRevision}</dd></dl>
    <label htmlFor={draftId}>선택한 필드의 수정 문구</label>
    <textarea id={draftId} value={props.workingText} disabled={props.inputDisabled} onChange={event => props.onDraftChange(event.currentTarget.value)}/>
    <Button disabled={props.saveDisabled} onClick={() => props.onSave()}>수정본 저장</Button>
    <Button disabled={props.acceptDisabled} onClick={() => props.onAccept()}>명시적 채택</Button>
    <Button disabled={props.refreshDisabled} onClick={() => props.onRefresh()}>저장 결과 다시 확인</Button>
    <p role="status">{props.errorText}</p>
  </section>;
}
