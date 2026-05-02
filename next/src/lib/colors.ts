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

// core はアルファベット順でコンテナ名を返すので、その順序の index を渡せば
// 先頭コンテナ = palette[0] = blue になり、N <= palette.length なら衝突しない。
// 新規コンテナの追加で alphabetical 位置が変わると色がシフトする点は妥協。
export const borderColorFor = (index: number): string =>
  palette[index % palette.length];

export const backgroundColorFor = (index: number): string =>
  hexToRgba(borderColorFor(index), 0.5);
