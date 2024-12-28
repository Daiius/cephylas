
export const revalidate = 10;


import Chart from '@/components/Chart';
import { readLogs } from '@/lib/logReader';


export default async function Home() {
  const logs = await readLogs();
  
  const datasets = Object.entries(logs)
    .map(([key, value]) => ({
      label: key,
      data: value,
      parsing: {
        xAxisKey: 'time',
        yAxisKey: 'cpu.percentage',
      },
    }));

  return (
    <div>
      {datasets &&
        <Chart
          datasets={datasets}
          title='CPU usage (%)'
        />
      }
      </div>
  );
}

