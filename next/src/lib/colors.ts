// Tableau 10 palette
// ref: https://www.tableau.com/blog/colors-upgrade-tableau-10-56782
const palette = [
  '#4e79a7', // blue
  '#f28e2b', // orange
  '#e15759', // red
  '#76b7b2', // teal
  '#59a14f', // green
  '#edc948', // yellow
  '#b07aa1', // purple
  '#ff9da7', // pink
  '#9c755f', // brown
  '#bab0ac', // gray
] as const;

const hexToRgba = (hex: string, alpha: number): string => {
  const h = hex.replace('#', '');
  const r = parseInt(h.substring(0, 2), 16);
  const g = parseInt(h.substring(2, 4), 16);
  const b = parseInt(h.substring(4, 6), 16);
  return `rgba(${r},${g},${b},${alpha})`;
};

// コードポイント順の比較。core の並び (Rust の String の順序) と同じ結果になる。
// `<` は UTF-16 のコード単位で比べるので、サロゲートペアを含む名前で順序が変わる。
// localeCompare はロケール依存で、大文字と小文字の並びも変わるので使わない。
const compareByCodePoint = (a: string, b: string): number => {
  const ia = a[Symbol.iterator]();
  const ib = b[Symbol.iterator]();
  for (;;) {
    const ca = ia.next();
    const cb = ib.next();
    if (ca.done || cb.done) return (ca.done ? 0 : 1) - (cb.done ? 0 : 1);
    const diff = ca.value.codePointAt(0)! - cb.value.codePointAt(0)!;
    if (diff !== 0) return diff;
  }
};

/**
 * コンテナ名を表示順 (コードポイント順) に並べた新しい配列を返す。重複は除く。
 * サーバ (データセットの並び) とクライアント (色の割り当て) の両方から使う。
 */
export const sortContainerNames = (names: Iterable<string>): string[] =>
  [...new Set(names)].sort(compareByCodePoint);

// 並べ替えた順序の index を渡せば、先頭コンテナ = palette[0] = blue になり、
// N <= palette.length なら衝突しない。index は画面全体の名前の一覧での位置で
// 決める (components/ContainerColorContext.tsx)。
// 新規コンテナの追加で alphabetical 位置が変わると色がシフトする点は妥協。
export const borderColorFor = (index: number): string =>
  palette[index % palette.length];

export const backgroundColorFor = (index: number): string =>
  hexToRgba(borderColorFor(index), 0.5);
