type Props = {};
export function Fixture(props: Props) {
  return <div className="grid grid-cols-1 gap-3 p-6 sm:grid-cols-2 lg:grid-cols-4"><div className="h-12 bg-red-400 md:bg-green-400"></div><div className="h-12 bg-red-400 md:bg-green-400"></div><div className="hidden h-12 bg-purple-500 sm:block"></div><div className="block h-12 bg-purple-300 lg:hidden"></div></div>;
}
