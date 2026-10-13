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

  // 並列フェッチ。core は接続ごとにスレッドを起こして並列に処理する
  // (同時 16 本まで。超えた分は断られず、枠が空くまで core 側で待つ)。
  const responses = await Promise.all(
    containerNames.map((name) => fetchMemoryStatus(name)),
  );
  const datasets: AppDataset[] = [];
  for (const [i, response] of responses.entries()) {
    if (!response.ok) return (<div>メモリ使用率取得中...</div>);
    datasets.push({
      containerName: containerNames[i],
      label: containerNames[i],
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
