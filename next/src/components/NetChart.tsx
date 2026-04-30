import { clsx } from 'clsx';
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
  const containersResponse = await fetchContainers();
  if (!containersResponse.ok) {
    return (<div>コンテナ名取得中...</div>);
  }
  const containerNames = containersResponse.data;

  const datasets: AppDataset[] = [];
  for (const [i, containerName] of containerNames.entries()) {
    const border = borderColorFor(i);
    const bg = backgroundColorFor(i);

    const responseRecv = await fetchNetStatus(containerName, 'recv');
    if (!responseRecv.ok) return (<div>Net recv 取得中...</div>);
    datasets.push({
      containerName,
      label: `${containerName} recv`,
      data: responseRecv.data,
      borderColor: border,
      backgroundColor: bg,
      borderDash: [1, 0],
    });

    const responseSend = await fetchNetStatus(containerName, 'send');
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
