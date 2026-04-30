
const palette = [
  'rgb( 54, 162, 235)', // blue
  'rgb(255,  99, 132)', // red
  'rgb(255, 159,  64)', // orange
  'rgb(255, 205,  86)', // yellow
  'rgb( 75, 192, 192)', // green
  'rgb(153, 102, 255)', // purple
  'rgb(201, 203, 207)', // grey
  'rgb(255, 105, 180)', // pink
] as const;

const hash = (s: string): number => {
  let h = 0;
  for (let i = 0; i < s.length; i++) {
    h = (h * 31 + s.charCodeAt(i)) | 0;
  }
  return Math.abs(h);
};

export const borderColorFor = (containerName: string): string =>
  palette[hash(containerName) % palette.length];

export const backgroundColorFor = (containerName: string): string =>
  borderColorFor(containerName)
    .replace('rgb(', 'rgba(')
    .replace(')', ',0.5)');
