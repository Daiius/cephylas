import clsx from 'clsx';
import { Chart, type AppDataset } from '@/components/Chart';
import { borderColorFor, backgroundColorFor } from '@/lib/colors';

import {
  fetchContainers,
  fetchCpuStatus,
} from '@/lib/fetchers';

export const CpuChart = async ({
  className,
}: {
  className?: string;
}) => {
  const containersResponse = await fetchContainers();
  if (!containersResponse.ok) { return (<div>コンテナ名取得中...</div>); }
  const containerNames = containersResponse.data;

  const datasets: AppDataset[] = [];
  for (const containerName of containerNames) {
    const response = await fetchCpuStatus(containerName);
    if (!response.ok) return (<div>CPU使用率取得中...</div>);
    datasets.push({
      containerName,
      label: containerName,
      data: response.data,
      borderColor: borderColorFor(containerName),
      backgroundColor: backgroundColorFor(containerName),
    });
  }

  return (
    <Chart
      className={clsx(className)}
      chartId='chartjs-cpu-usage'
      datasets={datasets}
      title='CPU usage (%)'
      yLabel='%'
    />
  );
};
