import { useState } from "react";
import { Button } from "@/components/ui/button";
type Props = { summaryLabel: string; detailsLabel: string; };
export function QueueFooter(props: Props) {
  const [expanded, setExpanded] = useState(false);
  return <footer className="aw-queue-footer"><div className="aw-queue-summary"><Button className="aw-queue-toggle" aria-expanded={expanded} aria-controls="studio-queue-details" onClick={() => setExpanded(previous => !previous)}><span className="aw-icon aw-icon-episodes" aria-hidden="true"/>작업 대기열</Button><span>{props.summaryLabel}</span></div><section id="studio-queue-details" className="aw-queue-details" hidden={!expanded} aria-label="작업 대기열 상세"><p>{props.detailsLabel}</p></section></footer>;
}
