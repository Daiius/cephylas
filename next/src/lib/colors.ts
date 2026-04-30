// Tableau 10 palette
// ref: https://www.tableau.com/blog/colors-upgrade-tableau-10-56782
const palette = [
  '#4e79a7', // blue
  '#59a14f', // green
  '#9c755f', // brown
  '#f28e2b', // orange
  '#edc948', // yellow
  '#bab0ac', // gray
  '#e15759', // red
  '#b07aa1', // purple
  '#76b7b2', // teal
  '#ff9da7', // pink
] as const;

const hash = (s: string): number => {
  let h = 0;
  for (let i = 0; i < s.length; i++) {
    h = (h * 31 + s.charCodeAt(i)) | 0;
  }
  return Math.abs(h);
};

const hexToRgba = (hex: string, alpha: number): string => {
  const h = hex.replace('#', '');
  const r = parseInt(h.substring(0, 2), 16);
  const g = parseInt(h.substring(2, 4), 16);
  const b = parseInt(h.substring(4, 6), 16);
  return `rgba(${r},${g},${b},${alpha})`;
};

export const borderColorFor = (containerName: string): string =>
  palette[hash(containerName) % palette.length];

export const backgroundColorFor = (containerName: string): string =>
  hexToRgba(borderColorFor(containerName), 0.5);
