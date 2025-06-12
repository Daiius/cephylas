import clsx from 'clsx';
import Chart from '@/components/Chart';

import {
  fetchContainers,
  fetchMemoryStatus,
  MemoryUsageDatasets,
} from '@/lib/fetchers';


export const CpuChart: React.FC<
  Omit<
    React.ComponentProps<typeof Chart>,
    "chartId" | "datasets" | "title"
  >
> = async ({
  className,
  ...props
}) => {
  const containersResponse = await fetchContainers();
  if (!containersResponse.ok) {
    return (<div>コンテナ名取得中...</div>);
  }
  const containerNames = containersResponse.data;

  const memoryUsageDatasets: MemoryUsageDatasets = [];
  for (const containerName of containerNames) {
    const response = await fetchMemoryStatus(containerName);
    if (!response.ok) return (<div>CPU使用率取得中...</div>);
    memoryUsageDatasets.push({
      label: containerName,
      data: response.data,
    });
  }

  return (
    <Chart 
      className={clsx(className)}
      {...props}
      chartId='chartjs-memory-usage'
      datasets={memoryUsageDatasets} 
      title='Memory usage (%)' 
    />
  );
};

export default CpuChart;

