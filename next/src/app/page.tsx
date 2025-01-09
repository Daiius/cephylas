
export const revalidate = 10;


import Chart from '@/components/Chart';
import { readLogs } from '@/lib/logReader';
import { downsampleByLTTB } from '@/lib/downsample';

type ElementType<T extends any[]> = 
  T extends (infer E)[] ? E : never;

type Log = Awaited<ReturnType<typeof readLogs>>;
type LogData = ElementType<Log[keyof Log]>;

const DownsampleCount = 512;

// could not find exported one,
// so i copied it from Chart.js source code.
const borderColors = [
  'rgb(54, 162, 235)', // blue
  'rgb(255, 99, 132)', // red
  'rgb(255, 159, 64)', // orange
  'rgb(255, 205, 86)', // yellow
  'rgb(75, 192, 192)', // green
  'rgb(153, 102, 255)', // purple
  'rgb(201, 203, 207)' // grey
];
const backgroundColors = borderColors
  .map(bc => bc.replace('rgb(', 'rgba(').replace(')', ',0.5)'));

const prepareDatasets = (
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

const prepareDatasetsIOandNet = (
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


export default async function Home() {

  const logs = await readLogs();

  if (Object.values(logs).length === 0) {
    return (<div>データ集計中...</div>);
  }

  console.time("データ成型");

  const datasetsCpu = prepareDatasets(
    logs, d => ({ x: d.time, y: d.cpu.percentage })
  );

  const datasetsMemory = prepareDatasets(
    logs, d => ({ x: d.time, y: d.memory.percentage })
  );
  const datasetsDisk = prepareDatasetsIOandNet(logs, 'io');

  const datasetsNet = prepareDatasetsIOandNet(logs, 'net');

  console.timeEnd("データ成型");

  return (
    <div>
      <Chart 
        chartId='chartjs-cpu-usage'
        datasets={datasetsCpu} title='CPU usage (%)' 
      />
      <Chart 
        chartId='chartjs-memory-usage'
        datasets={datasetsMemory} title='Memory usage (%)'
      />
      <Chart
        chartId='chartjs-disk-usage'
        datasets={datasetsDisk} title='Disk usage (kB/s)'
      />
      <Chart
        chartId='chartjs-net-usage'
        datasets={datasetsNet} title='Net usage (kB/s)'
      />
    </div>
  );
}

