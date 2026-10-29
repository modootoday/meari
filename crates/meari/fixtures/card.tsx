type Props = { label: string; disabled: boolean; };
export function Card(props: Props) {
  return (
    <section className="flex gap-2">
      <button disabled={props.disabled}>{props.label}</button>
      <Badge label={props.label} />
    </section>
  );
}
