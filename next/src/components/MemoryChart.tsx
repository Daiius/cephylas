import clsx from 'clsx';
import { cacheLife, cacheTag } from 'next/cache';
import { Chart, type AppDataset } from '@/components/Chart';
import { borderColorFor, backgroundColorFor } from '@/lib/colors';

import {
  fetchContainers,
  fetchMemoryStatus,
} from '@/lib/fetchers';

export const MemoryChart = async ({
  className,
}: {
  className?: string;
}) => {
  'use cache';
  cacheLife({ revalidate: 10, expire: 60, stale: 10 });
  cacheTag('chart:memory');

  const containersResponse = await fetchContainers();
  if (!containersResponse.ok) {
    return (<div>コンテナ名取得中...</div>);
  }
  const containerNames = containersResponse.data;

  const datasets: AppDataset[] = [];
  for (const [i, containerName] of containerNames.entries()) {
    const response = await fetchMemoryStatus(containerName);
    if (!response.ok) return (<div>メモリ使用率取得中...</div>);
    datasets.push({
      containerName,
      label: containerName,
      data: response.data,
      borderColor: borderColorFor(i),
      backgroundColor: backgroundColorFor(i),
    });
  }

  return (
    <Chart
      className={clsx(className)}
      chartId='chartjs-memory-usage'
      datasets={datasets}
      title='Memory usage (%)'
      yLabel='%'
    />
  );
};
