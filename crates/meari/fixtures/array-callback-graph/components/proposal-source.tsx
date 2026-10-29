type Props = {
  proposalId: string; revisionLabel: string; sourceIdentityLabel: string;
  sourceRefs: string[]; sourceText: string; generatedRaw: string; qualityLabel: string;
};

export function ProposalSource(props: Props) {
  return <section className="aw-inspector-section" aria-label="제안과 원문 근거">
    <dl className="aw-metadata"><dt>제안</dt><dd>{props.proposalId}</dd><dt>문서 버전</dt><dd>{props.revisionLabel}</dd><dt>원문 식별</dt><dd>{props.sourceIdentityLabel}</dd></dl>
    <h3>원문 참조</h3><ul>{props.sourceRefs.map(reference => <li key={reference}>{reference}</li>)}</ul>
    <p className="aw-evidence-body">{props.sourceText}</p>
    <details><summary>생성 원문 확인</summary><pre className="aw-evidence-body">{props.generatedRaw}</pre></details>
    <p className="aw-muted">{props.qualityLabel}</p>
  </section>;
}
