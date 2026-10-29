import { useId } from "react";
import { ProposalActions } from "../../components/proposal-actions";
import { ProposalSource } from "../../components/proposal-source";

type Props = {
  proposalId: string; revisionLabel: string; sourceIdentityLabel: string; sourceRefs: string[];
  sourceText: string; generatedRaw: string; qualityLabel: string; selectedCardLabel: string;
  front: string; back: string; canEdit: boolean; canSave: boolean; canAccept: boolean;
  busy: boolean; refreshRequired: boolean; resultLabel: string;
  onFrontChange: (text: string) => void; onBackChange: (text: string) => void;
  onSave: () => void; onAccept: () => void; onRefresh: () => void;
};

export function CardProposalPanel(props: Props) {
  const frontId = useId();
  const backId = useId();
  return <section aria-label="카드 문구 제안 검토">
    <div className="aw-inspector-heading"><h3>카드 문구 제안</h3><p>{props.selectedCardLabel}</p></div>
    <ProposalSource proposalId={props.proposalId} revisionLabel={props.revisionLabel} sourceIdentityLabel={props.sourceIdentityLabel} sourceRefs={props.sourceRefs} sourceText={props.sourceText} generatedRaw={props.generatedRaw} qualityLabel={props.qualityLabel}/>
    <section className="aw-inspector-section">
      <label htmlFor={frontId}>앞면 문구</label><textarea id={frontId} className="aw-text-input aw-draft-input" value={props.front} disabled={!props.canEdit || props.busy || props.refreshRequired} onChange={event => props.onFrontChange(event.currentTarget.value)}/>
      <label htmlFor={backId}>뒷면 문구</label><textarea id={backId} className="aw-text-input aw-draft-input" value={props.back} disabled={!props.canEdit || props.busy || props.refreshRequired} onChange={event => props.onBackChange(event.currentTarget.value)}/>
      <p className="aw-muted">문구 채택은 카드 이미지 저장이나 게시를 실행하지 않습니다.</p>
    </section>
    <ProposalActions canSave={props.canSave} canAccept={props.canAccept} busy={props.busy} refreshRequired={props.refreshRequired} resultLabel={props.resultLabel} onSave={props.onSave} onAccept={props.onAccept} onRefresh={props.onRefresh}/>
  </section>;
}
