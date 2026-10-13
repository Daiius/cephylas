import { clsx } from 'clsx';
import { cacheLife, cacheTag } from 'next/cache';
import { Chart, type AppDataset } from '@/components/Chart';

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

  // 色はここでは付けない (クライアント側で、画面全体の名前の一覧から決める)
  const datasets: AppDataset[] = [];
  for (const containerName of sortedContainerNames(usages)) {
    const { read, write } = usages[containerName];

    datasets.push({
      containerName,
      label: `${containerName} read`,
      data: toPoints(read),
      borderDash: [1, 0],
    });
    datasets.push({
      containerName,
      label: `${containerName} write`,
      data: toPoints(write),
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
