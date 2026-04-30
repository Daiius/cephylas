import { Suspense } from 'react';

import { CpuChart } from '@/components/CpuChart';
import { MemoryChart } from '@/components/MemoryChart';
import { IoChart } from '@/components/IoChart';
import { NetChart } from '@/components/NetChart';
import { ChartSkeleton } from '@/components/ChartSkeleton';
import { Header } from '@/components/Header';
import { FilterProvider } from '@/components/FilterContext';

export const dynamic = 'force-dynamic';

type SearchParams = Promise<{ hidden?: string | string[] }>;

export default async function Home({
  searchParams,
}: {
  searchParams: SearchParams;
}) {
  const sp = await searchParams;
  const hiddenParam = Array.isArray(sp.hidden) ? sp.hidden.join(',') : sp.hidden;

  return (
    <FilterProvider initialHiddenParam={hiddenParam}>
      <Header />
      <main className='px-4 pb-4'>
        <Suspense fallback={<ChartSkeleton />}>
          <CpuChart />
        </Suspense>
        <Suspense fallback={<ChartSkeleton />}>
          <MemoryChart />
        </Suspense>
        <Suspense fallback={<ChartSkeleton />}>
          <IoChart />
        </Suspense>
        <Suspense fallback={<ChartSkeleton />}>
          <NetChart />
        </Suspense>
      </main>
    </FilterProvider>
  );
}
