type Props = {};
export function Fixture(props: Props) {
  return <div className="flex gap-4 p-6"><button className="rounded-md bg-blue-600 px-4 py-2 text-white hover:bg-blue-700 focus:ring-2">Hover me</button><input className="w-48 rounded-md border border-zinc-300 px-3 py-2 focus:border-blue-500" defaultValue="focus" /><button className="rounded-md bg-zinc-200 px-4 py-2 text-zinc-400 disabled:opacity-50" disabled>Disabled</button></div>;
}
