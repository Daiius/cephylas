import { clsx } from 'clsx';
import { cacheLife, cacheTag } from 'next/cache';
import { Chart, type AppDataset } from '@/components/Chart';
import { borderColorFor, backgroundColorFor } from '@/lib/colors';

import {
  fetchContainers,
  fetchIoStatus,
} from '@/lib/fetchers';

export const IoChart = async ({
  className,
}: {
  className?: string;
}) => {
  'use cache';
  cacheLife({ revalidate: 10, expire: 60, stale: 10 });
  cacheTag('chart:io');

  const containersResponse = await fetchContainers();
  if (!containersResponse.ok) {
    return (<div>コンテナ名取得中...</div>);
  }
  const containerNames = containersResponse.data;

  // 並列フェッチ。read + write × N コンテナを全部同時に発射する。
  // core が単一スレッドな現状は逐次処理に degrade するが、
  // core をスレッド化した時点で web 側は無修正で恩恵を受ける。
  const [reads, writes] = await Promise.all([
    Promise.all(containerNames.map((name) => fetchIoStatus(name, 'read'))),
    Promise.all(containerNames.map((name) => fetchIoStatus(name, 'write'))),
  ]);

  const datasets: AppDataset[] = [];
  for (const [i, containerName] of containerNames.entries()) {
    const border = borderColorFor(i);
    const bg = backgroundColorFor(i);

    const responseRead = reads[i];
    if (!responseRead.ok) return (<div>IO read 取得中...</div>);
    datasets.push({
      containerName,
      label: `${containerName} read`,
      data: responseRead.data,
      borderColor: border,
      backgroundColor: bg,
      borderDash: [1, 0],
    });

    const responseWrite = writes[i];
    if (!responseWrite.ok) return (<div>IO write 取得中...</div>);
    datasets.push({
      containerName,
      label: `${containerName} write`,
      data: responseWrite.data,
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
