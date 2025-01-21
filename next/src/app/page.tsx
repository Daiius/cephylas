

//export const revalidate = 10;
export const dynamic = 'force-dynamic';

import Chart from '@/components/Chart';
import Header from '@/components/Header';


export default async function Home() {

  console.time("データ取得");
  const containersResponse = 
    await fetch('http://cephylas:7878/containers');
  if (!containersResponse.ok) {
    return (<div>コンテナ名取得中...</div>);
  }
  const containerNames = await containersResponse.json();
  console.timeEnd("データ取得");


  console.time("データ成型");

  const datasetsCpu: { 
    label: string;
    data: any[];
  }[] = [];
  for (const containerName of containerNames) {
    const cpuResponse = 
      await fetch(`http://cephylas:7878/containers/${containerName}/cpu`);
    if (!cpuResponse.ok) return (<div>CPU使用率取得中...</div>);
    const rawData = await cpuResponse.json();
    console.log("rawData: ", rawData);
    datasetsCpu.push({
      label: containerName,
      data: rawData.map((d: any) => ({ x: new Date(d.time), y: d.percentage }))
    });
  }
  console.log('%o', datasetsCpu);
  
  //const datasetsCpu = prepareDatasets(
  //  logs, d => ({ x: d.time, y: d.cpu.percentage })
  //);
  //const datasetsMemory = prepareDatasets(
  //  logs, d => ({ x: d.time, y: d.memory.percentage })
  //);
  //const datasetsDisk = prepareDatasetsIOandNet(logs, 'io');
  //const datasetsNet = prepareDatasetsIOandNet(logs, 'net');

  console.timeEnd("データ成型");


  return (
    <>
      <Header />
      <div className='px-4 pb-4'>
        <Chart 
          chartId='chartjs-cpu-usage'
          datasets={datasetsCpu} 
          title='CPU usage (%)' 
        />
        {/*
        <Chart 
          chartId='chartjs-memory-usage'
          datasets={datasetsMemory} 
          title='Memory usage (%)'
        />
        <Chart
          chartId='chartjs-disk-usage'
          datasets={datasetsDisk} 
          title='Disk usage (kB/s)'
        />
        <Chart
          chartId='chartjs-net-usage'
          datasets={datasetsNet} 
          title='Net usage (kB/s)'
        />
        */}
      </div>
    </>
  );
}

