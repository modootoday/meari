type Props = {};
export function Fixture(props: Props) {
  return <ul>{["a", "b"].map(item => <li key={item}>{item}</li>)}</ul>;
}
