// meari-visual: canonical-fonts
type Props = {};
export function Fixture(props: Props) {
  return (
    <div className="m-6 space-y-2 text-zinc-900">
      <p className="text-xl">Pretendard 기본 글꼴 Hangul 한글</p>
      <p className="text-xl font-semibold">Variable 600 굵게 Bold</p>
      <p className="text-lg">日本語のかな カナ</p>
      <p className="font-serif text-xl">Source Serif 4 canonical serif</p>
    </div>
  );
}
