import clsx from 'clsx';
import z from 'zod';
import Chart from '@/components/Chart';

// could not find exported one,
// so i copied it from Chart.js source code.
const borderColors = [
  'rgb( 54, 162, 235)', // blue
  'rgb(255,  99, 132)', // red
  'rgb(255, 159,  64)', // orange
  'rgb(255, 205,  86)', // yellow
  'rgb( 75, 192, 192)', // green
  'rgb(153, 102, 255)', // purple
  'rgb(201, 203, 207)', // grey
];
const backgroundColors = borderColors
  .map(bc => bc.replace('rgb(', 'rgba(').replace(')', ',0.5)'));

const IoUsageDataSchema = z.array(
  z.object({
    time: z.string().optional(),
    readkBps: z.number().nullish(),
    writekBps: z.number().nullish(),
  })
);
const IoReadDatasetSchema = IoUsageDataSchema.transform((data) => 
  data.map(d => ({ x: d.time, y: d.readkBps }))
);
const IoWriteDatasetSchema = IoUsageDataSchema.transform((data) =>
 data.map(d => ({ x: d.time, y: d.writekBps }))
);

type IoUsageDatasets = {
  label: string;
  data: z.infer<typeof IoReadDatasetSchema>;
  backgroundColor: string;
  borderColor: string;
  borderDash: [number, number];
}[];

export const IoChart: React.FC<
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

  const ioUsageDatasets: IoUsageDatasets = [];
  let icolor = 0;
  for (const containerName of containerNames) {
    const responseRead = 
      await fetch(`http://cephylas:7878/containers/${containerName}/io/read`);
    if (!responseRead.ok) return (<div>CPU使用率取得中...</div>);
    const rawDataRead = await responseRead.json();
    const validatedJsonRead = IoReadDatasetSchema.parse(rawDataRead);
    ioUsageDatasets.push({
      label: containerName + " read",
      data: validatedJsonRead,
      backgroundColor: 
        backgroundColors[icolor % backgroundColors.length],
      borderColor:
        borderColors[icolor % backgroundColors.length],
      borderDash: [1,0],
    });

    const responseWrite = 
      await fetch(`http://cephylas:7878/containers/${containerName}/io/write`);
    if (!responseWrite.ok) return (<div>CPU使用率取得中...</div>);
    const rawDataWrite = await responseWrite.json();
    const validatedJsonWrite = IoWriteDatasetSchema.parse(rawDataWrite);
    ioUsageDatasets.push({
      label: containerName + " write",
      data: validatedJsonWrite,
      backgroundColor: 
        backgroundColors[icolor % backgroundColors.length],
      borderColor:
        borderColors[icolor % backgroundColors.length],
      borderDash: [5,5],
    });

    icolor++;
  }

  return (
    <Chart 
      className={clsx(className)}
      {...props}
      chartId='chartjs-io-usage'
      datasets={ioUsageDatasets} 
      title='IO speeds (kBps)' 
    />
  );
};

export default IoChart;

