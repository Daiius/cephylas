import { clsx } from 'clsx';
import { cacheLife, cacheTag } from 'next/cache';
import { Chart, type AppDataset } from '@/components/Chart';
import { borderColorFor, backgroundColorFor } from '@/lib/colors';

import {
  fetchContainers,
  fetchNetStatus,
} from '@/lib/fetchers';

export const NetChart = async ({
  className,
}: {
  className?: string;
}) => {
  'use cache';
  cacheLife({ revalidate: 10, expire: 60, stale: 10 });
  cacheTag('chart:net');

  const containersResponse = await fetchContainers();
  if (!containersResponse.ok) {
    return (<div>コンテナ名取得中...</div>);
  }
  const containerNames = containersResponse.data;

  // 並列フェッチ。recv + send × N コンテナを全部同時に発射する。
  // core が単一スレッドな現状は逐次処理に degrade するが、
  // core をスレッド化した時点で web 側は無修正で恩恵を受ける。
  const [recvs, sends] = await Promise.all([
    Promise.all(containerNames.map((name) => fetchNetStatus(name, 'recv'))),
    Promise.all(containerNames.map((name) => fetchNetStatus(name, 'send'))),
  ]);

  const datasets: AppDataset[] = [];
  for (const [i, containerName] of containerNames.entries()) {
    const border = borderColorFor(i);
    const bg = backgroundColorFor(i);

    const responseRecv = recvs[i];
    if (!responseRecv.ok) return (<div>Net recv 取得中...</div>);
    datasets.push({
      containerName,
      label: `${containerName} recv`,
      data: responseRecv.data,
      borderColor: border,
      backgroundColor: bg,
      borderDash: [1, 0],
    });

    const responseSend = sends[i];
    if (!responseSend.ok) return (<div>Net send 取得中...</div>);
    datasets.push({
      containerName,
      label: `${containerName} send`,
      data: responseSend.data,
      borderColor: border,
      backgroundColor: bg,
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
