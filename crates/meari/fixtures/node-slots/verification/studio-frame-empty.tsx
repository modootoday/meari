import { StudioFrame } from "../studio/frame";

type Props = { productName: string; contextLabel: string; };

export function StudioFrameEmpty(props: Props) {
  return <StudioFrame productName={props.productName} contextLabel={props.contextLabel} navigation={null} workspace={null} inspector={null} queue={null}/>;
}
