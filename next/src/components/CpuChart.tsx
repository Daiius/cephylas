import clsx from 'clsx';
import z from 'zod';
import Chart from '@/components/Chart';

const CpuUsageDataSchema = z.array(
  z.object({
    time: z.string().optional(), 
    percentage: z.number().optional().nullish(),
  })
);
const CpuUsageDatasetSchema = CpuUsageDataSchema.transform((data) => 
  data.map(d => ({ x: d.time, y: d.percentage }))
);

type CpuUsageDatasets = {
  label: string;
  data: z.infer<typeof CpuUsageDatasetSchema>;
}[];

export const CpuChart: React.FC<
  Omit<
    React.ComponentProps<typeof Chart>,
    "chartId" | "datasets" | "title"
  >
> = async ({
  className,
  ...props
}) => {
  const containersResponse = 
    await fetch('http://cephylas:7878/containers');
  if (!containersResponse.ok) {
    return (<div>コンテナ名取得中...</div>);
  }
  const containerNames = await containersResponse.json();

  const cpuUsageDatasets: CpuUsageDatasets = [];
  for (const containerName of containerNames) {
    const response = 
      await fetch(`http://cephylas:7878/containers/${containerName}/cpu`);
    if (!response.ok) return (<div>CPU使用率取得中...</div>);
    const rawData = await response.json();
    const validatedJson = CpuUsageDatasetSchema.parse(rawData);
    cpuUsageDatasets.push({
      label: containerName,
      data: validatedJson,
    });
  }

  return (
    <Chart 
      className={clsx(className)}
      {...props}
      chartId='chartjs-cpu-usage'
      datasets={cpuUsageDatasets} 
      title='CPU usage (%)' 
    />
  );
};

export default CpuChart;

