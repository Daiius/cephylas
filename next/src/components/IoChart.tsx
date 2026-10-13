import { clsx } from 'clsx';
import { cacheLife, cacheTag } from 'next/cache';
import { Chart, type AppDataset } from '@/components/Chart';
import { borderColorFor, backgroundColorFor } from '@/lib/colors';

import {
  fetchUsages,
  sortedContainerNames,
  toPoints,
} from '@/lib/fetchers';

export const IoChart = async ({
  className,
}: {
  className?: string;
}) => {
  'use cache';
  cacheLife({ revalidate: 10, expire: 60, stale: 10 });
  cacheTag('chart:io');

  // 全コンテナ分の read と write を 1 リクエストで取る
  const response = await fetchUsages('io');
  if (!response.ok) return (<div>IO 取得中...</div>);
  const usages = response.data;

  const datasets: AppDataset[] = [];
  for (const [i, containerName] of sortedContainerNames(usages).entries()) {
    const border = borderColorFor(i);
    const bg = backgroundColorFor(i);
    const { read, write } = usages[containerName];

    datasets.push({
      containerName,
      label: `${containerName} read`,
      data: toPoints(read),
      borderColor: border,
      backgroundColor: bg,
      borderDash: [1, 0],
    });
    datasets.push({
      containerName,
      label: `${containerName} write`,
      data: toPoints(write),
      borderColor: border,
      backgroundColor: bg,
      borderDash: [5, 5],
    });
  }

  return (
    <Chart
      className={clsx(className)}
      chartId='chartjs-io-usage'
      datasets={datasets}
      title='IO speeds (kBps) — solid: read, dashed: write'
      yLabel='kBps'
    />
  );
};
