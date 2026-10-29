import { Counter } from "./counter";
type Props = { caption: string; disabled: boolean };
export function HookModules(props: Props) {
  return <main><Counter caption={props.caption} disabled={props.disabled}/><Counter caption={props.caption} disabled={props.disabled}/></main>;
}
