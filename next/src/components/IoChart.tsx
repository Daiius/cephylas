import { clsx } from 'clsx';
import { Chart } from '@/components/Chart';

import {
  fetchContainers,
  fetchIoStatus,
  IoUsageDatasets,
} from '@/lib/fetchers';

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

export const IoChart = async ({
  className,
}: {
  className?: string,
}) => {
  const containersResponse = await fetchContainers();
  if (!containersResponse.ok) {
    return (<div>コンテナ名取得中...</div>);
  }
  const containerNames = containersResponse.data;

  const ioUsageDatasets: IoUsageDatasets = [];
  let icolor = 0;
  for (const containerName of containerNames) {
    const responseRead = await fetchIoStatus(containerName, 'read');
    if (!responseRead.ok) return (<div>CPU使用率取得中...</div>);
    ioUsageDatasets.push({
      label: containerName + " read",
      data: responseRead.data,
      backgroundColor: 
        backgroundColors[icolor % backgroundColors.length],
      borderColor:
        borderColors[icolor % backgroundColors.length],
      borderDash: [1,0],
    });

    const responseWrite = await fetchIoStatus(containerName, 'write');
    if (!responseWrite.ok) return (<div>CPU使用率取得中...</div>);
    ioUsageDatasets.push({
      label: containerName + " write",
      data: responseWrite.data,
      backgroundColor: 
        backgroundColors[icolor % backgroundColors.length],
      borderColor:
        borderColors[icolor % backgroundColors.length],
      borderDash: [5,5],
    });

    icolor++;
  }

  return (
    <Chart 
      className={clsx(className)}
      chartId='chartjs-io-usage'
      datasets={ioUsageDatasets} 
      title='IO speeds (kBps)' 
    />
  );
};

