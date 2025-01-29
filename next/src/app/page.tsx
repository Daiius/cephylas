import React from 'react';

//export const revalidate = 10;
export const dynamic = 'force-dynamic';

import CpuChart from '@/components/CpuChart';
import MemoryChart from '@/components/MemoryChart';
import IoChart from '@/components/IoChart';
import NetChart from '@/components/NetChart';
import ChartSkeleton from '@/components/ChartSkeleton';
import Header from '@/components/Header';


export default async function Home() {

  return (
    <>
      <Header />
      <div className='px-4 pb-4'>
        <React.Suspense fallback={<ChartSkeleton />}>
          <CpuChart />
        </React.Suspense>
        <React.Suspense fallback={<ChartSkeleton />}>
          <MemoryChart />
        </React.Suspense>
        <React.Suspense fallback={<ChartSkeleton />}>
          <IoChart />
        </React.Suspense>
        <React.Suspense fallback={<ChartSkeleton />}>
          <NetChart />
        </React.Suspense>
      </div>
    </>
  );
}

