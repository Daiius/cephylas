export const dynamic = 'force-dynamic';

import Chart from '@/components/Chart';
import Header from '@/components/Header';

import { readLogs } from '@/lib/logReader';
import {
  prepareDatasets,
  prepareDatasetsIOandNet,
} from '@/lib/datasets';

import { trpc } from '@/trpc';

export default async function Home() {

  const logs = await readLogs();

  if (Object.values(logs).length === 0) {
    return (<div>データ集計中...</div>);
  }

  console.time('データ成型');

  const datasetsCpu = prepareDatasets(
    logs, d => ({ x: d.time, y: d.cpu.percentage })
  );
  const datasetsMemory = prepareDatasets(
    logs, d => ({ x: d.time, y: d.memory.percentage })
  );
  const datasetsDisk = prepareDatasetsIOandNet(logs, 'io');
  const datasetsNet = prepareDatasetsIOandNet(logs, 'net');

  console.timeEnd('データ成型');

  console.time('trpc call');
  await trpc.stats.query();
  console.timeEnd('trpc call');

  return (
    <>
      <Header />
      <div className='px-4 pb-4'>
        <Chart 
          chartId='chartjs-cpu-usage'
          datasets={datasetsCpu} 
          title='CPU usage (%)' 
        />
        <Chart 
          chartId='chartjs-memory-usage'
          datasets={datasetsMemory} 
          title='Memory usage (%)'
        />
        <Chart
          chartId='chartjs-disk-usage'
          datasets={datasetsDisk} 
          title='Disk usage (kB/s)'
        />
        <Chart
          chartId='chartjs-net-usage'
          datasets={datasetsNet} 
          title='Net usage (kB/s)'
        />
      </div>
    </>
  );
}

