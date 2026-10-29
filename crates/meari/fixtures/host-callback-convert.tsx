import { Button } from "@/components/ui/button";
type Props = { canConvert: boolean; onConvert: () => void; resultLabel: string; };
export function SubtitleConvertIntent(props: Props) {
  return <section aria-label="자막 변환 호출"><Button disabled={!props.canConvert} onClick={() => props.onConvert()}>새 파일로 변환</Button><p role="status">{props.resultLabel}</p></section>;
}
