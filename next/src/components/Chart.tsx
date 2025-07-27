'use client'

import { clsx } from 'clsx'
import {
  useEffect,
  useRef,
  useState,
} from 'react'

// Chart.js v4 あたりから
// 必要なモジュールのみをimport出来る様に変更された
// 多少はメモリ消費が減る...???
// 全部importする場合には以下でOK
import { Chart as ChartJs } from 'chart.js/auto';
import 'chartjs-adapter-luxon';

export type ChartProps = { 
  chartId: string,
  datasets: any,
  title?: string,
  className?: string,
}

export const Chart = ({
  chartId,
  datasets,
  title,
  className,
}: ChartProps) => {

  const [mounted, setMounted] = useState<boolean>(false);
  const refCanvas = useRef<HTMLCanvasElement|null>(null);
  const refChart = useRef<ChartJs|null>(null);

  useEffect(() => {
    if (mounted) {
      refCanvas.current =
        document.getElementById(chartId ?? 'chartjs-canvas') as HTMLCanvasElement;
      if (refCanvas.current == null) {
        throw new Error('Chart.js container is null.');
      }

      console.log('datasets: ', datasets);
      refChart.current = new ChartJs(
        refCanvas.current,
        {
          type: 'line',
          data: { datasets: datasets == null ? {} : datasets },
          options: {
            animation: false,
            //aspectRatio: 1,
            maintainAspectRatio: false,
            plugins: {
              title: {
                display: true,
                text: title,
              },
            },
            elements: {
              point: { radius: 0 },
              line: { borderWidth: 2 },
            },
            scales: {
              x: {
                type: 'time',
                time: { 
                  unit: 'minute',
                },
              },
              y: { min: 0 },
            }
          }
        }
      );
    } else { 
      setMounted(true); 
    }

    return () => refChart.current?.destroy();
  }, [mounted, datasets]);

  return (
    <div className='w-full h-[80vh]'>
      <canvas 
        id={chartId ?? 'chartjs-canvas'}
        className={clsx(
          className,
        )}
      >
      </canvas>
    </div>
  )
};

