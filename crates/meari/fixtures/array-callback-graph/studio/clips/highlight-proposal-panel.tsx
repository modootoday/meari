import { useId } from "react";
import { ProposalActions } from "../../components/proposal-actions";
import { ProposalSource } from "../../components/proposal-source";

type Props = {
  proposalId: string; revisionLabel: string; sourceIdentityLabel: string; sourceRefs: string[];
  sourceText: string; generatedRaw: string; qualityLabel: string; selectedCandidateLabel: string;
  title: string; rangeLabel: string; canEdit: boolean; canSave: boolean; canAccept: boolean;
  busy: boolean; refreshRequired: boolean; resultLabel: string;
  onTitleChange: (text: string) => void; onSave: () => void; onAccept: () => void; onRefresh: () => void;
};

export function HighlightProposalPanel(props: Props) {
  const titleId = useId();
  return <section aria-label="하이라이트 제안 검토">
    <div className="aw-inspector-heading"><h3>하이라이트 제안</h3><p>{props.selectedCandidateLabel}</p></div>
    <ProposalSource proposalId={props.proposalId} revisionLabel={props.revisionLabel} sourceIdentityLabel={props.sourceIdentityLabel} sourceRefs={props.sourceRefs} sourceText={props.sourceText} generatedRaw={props.generatedRaw} qualityLabel={props.qualityLabel}/>
    <section className="aw-inspector-section">
      <label htmlFor={titleId}>후보 제목</label><textarea id={titleId} className="aw-text-input aw-draft-input" value={props.title} disabled={!props.canEdit || props.busy || props.refreshRequired} onChange={event => props.onTitleChange(event.currentTarget.value)}/>
      <dl className="aw-metadata"><dt>원본 시간 연결</dt><dd>{props.rangeLabel}</dd></dl>
      <p className="aw-muted">제안 채택과 영상 구간 확정·작업 추가는 별도 동작입니다.</p>
    </section>
    <ProposalActions canSave={props.canSave} canAccept={props.canAccept} busy={props.busy} refreshRequired={props.refreshRequired} resultLabel={props.resultLabel} onSave={props.onSave} onAccept={props.onAccept} onRefresh={props.onRefresh}/>
  </section>;
}
