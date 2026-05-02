import clsx from 'clsx';
import { cacheLife, cacheTag } from 'next/cache';
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
  'use cache';
  cacheLife({ revalidate: 10, expire: 60, stale: 10 });
  cacheTag('chart:cpu');

  const containersResponse = await fetchContainers();
  if (!containersResponse.ok) { return (<div>コンテナ名取得中...</div>); }
  const containerNames = containersResponse.data;

  // 並列フェッチ。core が単一スレッドな現状は逐次処理に degrade するが、
  // core をスレッド化した時点で web 側は無修正で恩恵を受ける。
  const responses = await Promise.all(
    containerNames.map((name) => fetchCpuStatus(name)),
  );
  const datasets: AppDataset[] = [];
  for (const [i, response] of responses.entries()) {
    if (!response.ok) return (<div>CPU使用率取得中...</div>);
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
      chartId='chartjs-cpu-usage'
      datasets={datasets}
      title='CPU usage (%)'
      yLabel='%'
    />
  );
};
