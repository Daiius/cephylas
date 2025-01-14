

//export const revalidate = 10;
export const dynamic = 'force-dynamic';

import Chart from '@/components/Chart';
import Header from '@/components/Header';

import { readLogs } from '@/lib/logReader';
import {
  prepareDatasets,
  prepareDatasetsIOandNet,
} from '@/lib/datasets';

export default async function Home() {

  const response = await fetch('http://cephylas:7878');
  if (!response.ok) {
    return (<div>データ集計中...</div>);
  }
  const logs = JSON.parse(
    await response.text(),
    (key, value) => key === 'time' ? new Date(value) : value,
  );


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

