'use client'

import React from 'react';
import clsx from 'clsx';

// Chart.js v4 あたりから
// 必要なモジュールのみをimport出来る様に変更された
// 多少はメモリ消費が減る...???
// 全部importする場合には以下でOK
// import { Chart as ChartJs } from 'chart.js/auto';
// 正直、ちゃんと動いてるか見た目を確認しながら
// 追加していかないといけないので微妙
// TODO 効果の検証...
import {
  Chart as ChartJs,
  LineController,
  LineElement,
  PointElement,
  TimeScale,
  LinearScale,
  Legend,
  Colors,
} from 'chart.js';
ChartJs.register(
  LineController,
  LineElement,
  PointElement,
  TimeScale,
  LinearScale,
  Legend,
  Colors,
);
import 'chartjs-adapter-luxon';


const Chart: React.FC<
  React.ComponentProps<'canvas'>
  & { 
    chartId: string,
    datasets: any,
    title?: string,
    xlabel?: string,
    ylabel?: string,
  }
> = ({
  chartId,
  datasets,
  title,
  xlabel,
  ylabel,
  className,
  ...props
}) => {

  const [mounted, setMounted] = React.useState<boolean>(false);
  const refCanvas = 
    React.useRef<HTMLCanvasElement|null>(null);
  const refChart = 
    React.useRef<ChartJs|null>(null);

  React.useEffect(() => {
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
    <canvas 
      id={chartId ?? 'chartjs-canvas'}
      className={clsx(className)}
      {...props}
    >
    </canvas>
  )
};

export default Chart;
//export type { ChartData } from 'chart.js/auto';


