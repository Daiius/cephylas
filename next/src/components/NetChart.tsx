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

const NetUsageDataSchema = z.array(
  z.object({
    time: z.string().nullish(),
    recvkBps: z.number().nullish(),
    sendkBps: z.number().nullish(),
  })
);
const NetRecvDatasetSchema = NetUsageDataSchema.transform((data) => 
  data.map(d => ({ x: d.time, y: d.recvkBps }))
);
const NetSendDatasetSchema = NetUsageDataSchema.transform((data) =>
 data.map(d => ({ x: d.time, y: d.sendkBps }))
);

type NetUsageDatasets = {
  label: string;
  data: z.infer<typeof NetRecvDatasetSchema>;
  backgroundColor: string;
  borderColor: string;
  borderDash: [number, number];
}[];

export const NetChart: React.FC<
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

  const netUsageDatasets: NetUsageDatasets = [];
  let icolor = 0;
  for (const containerName of containerNames) {
    const responseRecv = 
      await fetch(`http://cephylas:7878/containers/${containerName}/net/recv`);
    if (!responseRecv.ok) return (<div>CPU使用率取得中...</div>);
    const rawDataRecv = await responseRecv.json();
    const validatedJsonRecv = NetRecvDatasetSchema.parse(rawDataRecv);
    netUsageDatasets.push({
      label: containerName + " recv",
      data: validatedJsonRecv,
      backgroundColor: 
        backgroundColors[icolor % backgroundColors.length],
      borderColor:
        borderColors[icolor % backgroundColors.length],
      borderDash: [1,0],
    });

    const responseSend = 
      await fetch(`http://cephylas:7878/containers/${containerName}/net/send`);
    if (!responseSend.ok) return (<div>CPU使用率取得中...</div>);
    const rawDataSend = await responseSend.json();
    const validatedJsonSend = NetSendDatasetSchema.parse(rawDataSend);
    netUsageDatasets.push({
      label: containerName + " send",
      data: validatedJsonSend,
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
      chartId='chartjs-net-usage'
      datasets={netUsageDatasets} 
      title='Net speeds (kBps)' 
    />
  );
};

export default NetChart;

