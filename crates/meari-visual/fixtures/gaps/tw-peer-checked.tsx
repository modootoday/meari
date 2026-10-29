type Props = {};
export function Fixture(props: Props) {
  return (
    <label className="flex items-center gap-2 p-6">
      <input className="peer size-4" type="checkbox" defaultChecked />
      <span className="text-zinc-500 peer-checked:font-semibold peer-checked:text-blue-600">Checked label</span>
    </label>
  );
}
