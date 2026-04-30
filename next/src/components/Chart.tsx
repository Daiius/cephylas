'use client'

import { clsx } from 'clsx'
import { useEffect, useRef } from 'react'

import { Chart as ChartJs, type ChartDataset } from 'chart.js/auto';
import 'chartjs-adapter-luxon';

import { useFilter } from './FilterContext';
import { MiniLegend } from './MiniLegend';

// 時間軸チャートの 1 点。Chart.js の Point は { x: number; y: number } だが
// time scale だと文字列を runtime で受け付けるので独自に定義する。
export type TimedPoint = { x?: string; y?: number | null };
type TimedSeries = TimedPoint[];

// containerName は src/types/chartjs.d.ts の declaration merging で
// ChartDatasetProperties 自体に生えているので、ここで再宣言は不要。
export type AppDataset = ChartDataset<'line', TimedSeries>;

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
  const refChart = useRef<ChartJs<'line', TimedSeries> | null>(null);
  const { hidden } = useFilter();

  useEffect(() => {
    if (!refCanvas.current) return;

    refChart.current = new ChartJs<'line', TimedSeries>(refCanvas.current, {
      type: 'line',
      data: {
        datasets: datasets.map((d) => ({
          ...d,
          hidden: d.containerName ? hidden.has(d.containerName) : false,
        })),
      },
      options: {
        animation: false,
        maintainAspectRatio: false,
        plugins: {
          // タイトルは HTML 側で出してミニ凡例の上に置く。
          title: { display: false },
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
      d.hidden = d.containerName ? hidden.has(d.containerName) : false;
    });
    chart.update('none');
  }, [hidden]);

  return (
    <div className='w-full mb-2'>
      {title && (
        <h3 className='text-sm font-semibold px-2 pt-2'>{title}</h3>
      )}
      <MiniLegend datasets={datasets} />
      <div className='w-full h-[75svh]'>
        <canvas
          ref={refCanvas}
          id={chartId}
          className={clsx(className)}
        />
      </div>
    </div>
  );
};
