import { clsx } from 'clsx';
import { cacheLife, cacheTag } from 'next/cache';
import { Chart, type AppDataset } from '@/components/Chart';

import {
  fetchUsages,
  sortedContainerNames,
  toPoints,
} from '@/lib/fetchers';

export const NetChart = async ({
  className,
}: {
  className?: string;
}) => {
  'use cache';
  cacheLife({ revalidate: 10, expire: 60, stale: 10 });
  cacheTag('chart:net');

  // 全コンテナ分の recv と send を 1 リクエストで取る
  const response = await fetchUsages('net');
  if (!response.ok) return (<div>Net 取得中...</div>);
  const usages = response.data;

  // 色はここでは付けない (クライアント側で、画面全体の名前の一覧から決める)
  const datasets: AppDataset[] = [];
  for (const containerName of sortedContainerNames(usages)) {
    const { recv, send } = usages[containerName];

    datasets.push({
      containerName,
      label: `${containerName} recv`,
      data: toPoints(recv),
      borderDash: [1, 0],
    });
    datasets.push({
      containerName,
      label: `${containerName} send`,
      data: toPoints(send),
      borderDash: [5, 5],
    });
  }

  return (
    <Chart
      className={clsx(className)}
      chartId='chartjs-net-usage'
      datasets={datasets}
      title='Net speeds (kBps) — solid: recv, dashed: send'
      yLabel='kBps'
    />
  );
};
