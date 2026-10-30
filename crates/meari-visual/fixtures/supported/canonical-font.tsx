// meari-visual: canonical-fonts
type Props = {};
export function Fixture(props: Props) {
  return (
    <div className="m-6 space-y-2 text-zinc-900">
      <p className="text-xl">Noto Sans 기본 글꼴 Hangul 한글</p>
      <p className="text-xl font-semibold">Variable 600 굵게 Bold</p>
      <p className="text-lg">日本語のかな カナ</p>
      <p className="text-lg font-bold">日本語の太字 カナ</p>
      <p className="font-serif text-xl">Noto Serif canonical serif</p>
      <p className="font-serif text-xl">노토 명조 한글 명조체</p>
    </div>
  );
}
