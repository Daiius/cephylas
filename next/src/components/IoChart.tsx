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

  const datasets: AppDataset[] = [];
  for (const [i, containerName] of containerNames.entries()) {
    const border = borderColorFor(i);
    const bg = backgroundColorFor(i);

    const responseRead = await fetchIoStatus(containerName, 'read');
    if (!responseRead.ok) return (<div>IO read 取得中...</div>);
    datasets.push({
      containerName,
      label: `${containerName} read`,
      data: responseRead.data,
      borderColor: border,
      backgroundColor: bg,
      borderDash: [1, 0],
    });

    const responseWrite = await fetchIoStatus(containerName, 'write');
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
