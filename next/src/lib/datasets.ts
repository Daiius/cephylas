
import { downsampleByLTTB } from '@/lib/downsample';
import { readLogs } from '@/lib/logReader';

type ElementType<T extends any[]> = 
  T extends (infer E)[] ? E : never;

type Log = Awaited<ReturnType<typeof readLogs>>;
type LogData = ElementType<Log[keyof Log]>;

const DownsampleCount = 512;

// could not find exported one,
// so i copied it from Chart.js source code.
const borderColors = [
  'rgb( 54, 162, 235)', // blue
  'rgb(255,  99, 132)', // red
  'rgb(255, 159,  64)', // orange
  'rgb(255, 205,  86)', // yellow
  'rgb( 75, 192, 192)', // green
  'rgb(153, 102, 255)', // purple
  'rgb(201, 203, 207)', // grey
];
const backgroundColors = borderColors
  .map(bc => bc.replace('rgb(', 'rgba(').replace(')', ',0.5)'));


export const prepareDatasets = (
  logs: Log, 
  xy: (d: LogData) => { x: Date | null, y: number | null},
) => Object.entries(logs)
  // コンテナ名順に並べ替え
  .sort(([labelA, _vA], [labelB, _vB]) => labelA.localeCompare(labelB))
  // Chart.js向けにデータ成型 & downsample
  .map(([label, data]) => ({
    label, 
    data: 
      data == null
      ? []
      : downsampleByLTTB(
        data.map(d => xy(d)),
        DownsampleCount,
        d => d.x?.getTime() ?? null, d => d.y,
        ),
  }));

const DataLabelPresetsIOandNet = {
  'io': {
    appendix: ['read', 'write'],
    xy: {
      'read': (d: LogData) => ({ 
        x: d.time, 
        y: d.io.readkBps,
      }),
      'write': (d: LogData) => ({ 
        x: d.time, 
        y: d.io.writekBps
      }),
    },
  },
  'net': {
    appendix: ['recv', 'send'],
    xy: {
      'recv': (d: LogData) => ({ x: d.time, y: d.net.recvkBps }),
      'send': (d: LogData) => ({ x: d.time, y: d.net.sendkBps }),
    }
  }
};

export const prepareDatasetsIOandNet = (
  logs: Log,
  ioOrNet: keyof typeof DataLabelPresetsIOandNet,
) =>  Object.entries(logs)
  // コンテナ名順にソート
  .sort(([labelA, _vA], [labelB, _vB]) => labelA.localeCompare(labelB))
  // recv, send  or  read, write  毎にdatasetsを生成
  .flatMap(([label, data], ilabel) => 
    // Chart.js向けにデータ成型
    DataLabelPresetsIOandNet[ioOrNet]
    .appendix
    .map(type => ({
      label: `${label} ${type}`, 
      data: downsampleByLTTB(
        data.map(d => 
          (DataLabelPresetsIOandNet[ioOrNet].xy as any)[type](d)
        ),
        DownsampleCount,
        d => d.x?.getTime() ?? null, d => d.y,
      ),
      borderColor: borderColors[ilabel % borderColors.length],
      backgroundColor: backgroundColors[ilabel % backgroundColors.length],
      borderDash: 
        type === "send" || type === "write"
        ? [5,5] 
        : [1,0],
    }))
  );


