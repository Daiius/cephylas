'use client'

import React from 'react';
import clsx from 'clsx';

import {
  Chart as ChartJs,
} from 'chart.js/auto';
import 'chartjs-adapter-luxon';


const Chart: React.FC<
  React.ComponentProps<'canvas'>
  & { 
    datasets: any,
    title?: string,
    xlabel?: string,
    ylabel?: string,
  }
> = ({
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
        document.getElementById('chartjs-canvas') as HTMLCanvasElement;
      if (refCanvas.current == null) {
        throw new Error('Chart.js container is null.');
      }

      console.log('datasets: ', datasets);
      refChart.current = new ChartJs(
        refCanvas.current,
        {
          type: 'line',
          data: { datasets },
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
      id='chartjs-canvas'
      className={clsx(className)}
      {...props}
    >
    </canvas>
  )
};

export default Chart;
export type { ChartData } from 'chart.js/auto';


