
export const revalidate = 10;


import Chart from '@/components/Chart';
import { readLogs } from '@/lib/logReader';

export default async function Home() {

  const logs = await readLogs();

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
  
  const datasetsCpu = Object.entries(logs)
    .map(([label, data]) => ({
      label, data, parsing: { xAxisKey: 'time', yAxisKey: 'cpu.percentage', },
    }));
  const datasetsMemory = Object.entries(logs)
    .map(([label, data]) => ({
      label, data, parsing: { xAxisKey: 'time', yAxisKey: 'memory.percentage', },
    }));
  const datasetsDisk = Object.entries(logs)
    .flatMap(([label, data], ilabel) => ["read", "write"].map(type => ({
      label: `${label} ${type}`, data, parsing: { xAxisKey: 'time', yAxisKey: `io.${type}kBps` },
      borderColor: borderColors[ilabel & borderColors.length],
      backgroundColor: backgroundColors[ilabel % backgroundColors.length],
      borderDash: type === "read" ? [5,5] : [1,0],
    })));
  const datasetsNet = Object.entries(logs)
    .flatMap(([label, data], ilabel) => ["send", "recv"].map(type => ({
      label: `${label} ${type}`, data, parsing: { xAxisKey: 'time', yAxisKey: `net.${type}kBps` },
      borderColor: borderColors[ilabel % borderColors.length],
      backgroundColor: backgroundColors[ilabel % backgroundColors.length],
      borderDash: type === "send" ? [5,5] : [1,0],
    })));


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

