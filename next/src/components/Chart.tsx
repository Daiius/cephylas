'use client'

import { clsx } from 'clsx'
import { useEffect, useRef } from 'react'

import { Chart as ChartJs, type ChartDataset } from 'chart.js/auto';
import 'chartjs-adapter-luxon';

import { useFilter } from './FilterContext';

export type AppDataset = ChartDataset<'line', { x?: string; y?: number | null }[]> & {
  containerName: string;
};

export type ChartProps = {
  chartId: string;
  datasets: AppDataset[];
  title?: string;
  yLabel?: string;
  className?: string;
};

export const Chart = ({
  chartId,
  datasets,
  title,
  yLabel,
  className,
}: ChartProps) => {
  const refCanvas = useRef<HTMLCanvasElement | null>(null);
  const refChart = useRef<ChartJs<'line'> | null>(null);
  const { hidden } = useFilter();

  useEffect(() => {
    if (!refCanvas.current) return;

    refChart.current = new ChartJs(refCanvas.current, {
      type: 'line',
      data: {
        datasets: datasets.map((d) => ({
          ...d,
          hidden: hidden.has(d.containerName),
        })),
      },
      options: {
        animation: false,
        maintainAspectRatio: false,
        plugins: {
          title: { display: !!title, text: title },
          legend: { display: false },
        },
        elements: {
          point: { radius: 0 },
          line: { borderWidth: 2 },
        },
        scales: {
          x: { type: 'time', time: { unit: 'minute' } },
          y: {
            min: 0,
            title: yLabel ? { display: true, text: yLabel } : undefined,
          },
        },
      },
    });

    return () => {
      refChart.current?.destroy();
      refChart.current = null;
    };
    // datasets を依存に入れると毎回 destroy/create される。
    // datasets はサーバから new array で来るので、本来は安定化したいが
    // PR 1 では動作優先で再生成を許容する。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [datasets]);

  // hidden の変更だけは update('none') で軽く反映
  useEffect(() => {
    const chart = refChart.current;
    if (!chart) return;
    chart.data.datasets.forEach((d) => {
      const containerName = (d as AppDataset).containerName;
      d.hidden = hidden.has(containerName);
    });
    chart.update('none');
  }, [hidden]);

  return (
    <div className='w-full h-[80svh]'>
      <canvas
        ref={refCanvas}
        id={chartId}
        className={clsx(className)}
      />
    </div>
  );
};
