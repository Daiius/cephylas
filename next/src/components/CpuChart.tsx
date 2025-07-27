import clsx from 'clsx';
import { Chart } from '@/components/Chart';

import {
  fetchContainers,
  fetchCpuStatus,
  CpuUsageDatasets,
} from '@/lib/fetchers';


export type CpuChartProps = {
  className?: string;
}

export const CpuChart = async ({
  className,
}: CpuChartProps) => {
  const containersResponse = await fetchContainers();
  if (!containersResponse.ok) { return (<div>コンテナ名取得中...</div>); }
  const containerNames = containersResponse.data;

  const cpuUsageDatasets: CpuUsageDatasets = [];
  for (const containerName of containerNames) {
    const response = await fetchCpuStatus(containerName);
    if (!response.ok) return (<div>CPU使用率取得中...</div>);
    cpuUsageDatasets.push({
      label: containerName,
      data: response.data,
    });
  }

  return (
    <Chart 
      className={clsx(className)}
      chartId='chartjs-cpu-usage'
      datasets={cpuUsageDatasets} 
      title='CPU usage (%)' 
    />
  );
};

