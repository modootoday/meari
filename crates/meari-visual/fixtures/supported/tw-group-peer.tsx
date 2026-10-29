type Props = {};
export function Fixture(props: Props) {
  return (
    <div className="space-y-4 p-6">
      <div className="group block w-64 rounded-lg border border-zinc-200 p-4 hover:bg-sky-500">
        <p className="font-semibold text-zinc-900 group-hover:text-white">New project</p>
        <p className="text-sm text-zinc-500 group-hover:text-white">Create from a template</p>
      </div>
      <div className="flex items-center gap-2">
        <span className="peer rounded bg-zinc-100 px-2 is-on">peer</span>
        <span className="text-zinc-500 peer-[.is-on]:font-semibold peer-[.is-on]:text-blue-600">peer-[.is-on] sibling</span>
      </div>
      <div className="group is-open w-64 rounded border border-zinc-200 p-2">
        <p className="text-zinc-400 group-[.is-open]:text-emerald-700">Arbitrary group</p>
      </div>
    </div>
  );
}
