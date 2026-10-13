import clsx from 'clsx';
import { cacheLife, cacheTag } from 'next/cache';
import { Chart, type AppDataset } from '@/components/Chart';
import { borderColorFor, backgroundColorFor } from '@/lib/colors';

import {
  fetchUsages,
  sortedContainerNames,
  toPoints,
} from '@/lib/fetchers';

export const MemoryChart = async ({
  className,
}: {
  className?: string;
}) => {
  'use cache';
  cacheLife({ revalidate: 10, expire: 60, stale: 10 });
  cacheTag('chart:memory');

  // 全コンテナ分を 1 リクエストで取る
  const response = await fetchUsages('memory');
  if (!response.ok) return (<div>メモリ使用率取得中...</div>);
  const usages = response.data;

  const datasets: AppDataset[] = sortedContainerNames(usages).map(
    (containerName, i) => ({
      containerName,
      label: containerName,
      data: toPoints(usages[containerName].percentage),
      borderColor: borderColorFor(i),
      backgroundColor: backgroundColorFor(i),
    }),
  );

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
