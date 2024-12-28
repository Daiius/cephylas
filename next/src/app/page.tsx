
export const revalidate = 10;


import Chart from '@/components/Chart';
import { readLogs } from '@/lib/logReader';


export default async function Home() {
  const logs = await readLogs();
  
  const datasetsCpu = Object.entries(logs)
    .map(([key, value]) => ({
      label: key, data: value,
      parsing: { xAxisKey: 'time', yAxisKey: 'cpu.percentage', },
    }));
  const datasetsMemory = Object.entries(logs)
    .map(([key, value]) => ({
      label: key, data: value,
      parsing: { xAxisKey: 'time', yAxisKey: 'memory.percentage', },
    }));

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
    </div>
  );
}

