type Props = {};
export function Fixture(props: Props) {
  return <main className="flex flex-col gap-4 p-6"><div className="flex flex-row items-center justify-between gap-2"><div className="h-12 w-24 bg-red-500"></div><div className="h-8 w-16 bg-green-500"></div><div className="h-16 w-20 bg-blue-500"></div></div><div className="flex flex-wrap gap-3"><div className="h-10 w-32 bg-amber-400"></div><div className="h-10 w-32 bg-violet-500"></div><div className="h-10 w-32 bg-teal-500"></div><div className="h-10 w-32 bg-pink-500"></div></div></main>;
}
