'use client'

import { clsx } from 'clsx'
import { useEffect, useMemo, useRef } from 'react'

import { Chart as ChartJs, type ChartDataset } from 'chart.js/auto';
import 'chartjs-adapter-luxon';

import { useContainerColors } from './ContainerColorContext';
import { useFilter } from './FilterContext';
import { MiniLegend } from './MiniLegend';

// 時間軸チャートの 1 点。x はエポックからのミリ秒 (time scale は数値を受け取れる)。
// Chart.js の Point は { x: number; y: number } だが、欠損を null で持つので
// 独自に定義する。
export type TimedPoint = { x: number; y: number | null };
type TimedSeries = TimedPoint[];

// containerName は src/types/chartjs.d.ts の declaration merging で
// ChartDatasetProperties 自体に生えているので、ここで再宣言は不要。
// 色 (borderColor / backgroundColor) はサーバからは渡さない。このコンポーネントが
// ContainerColorContext から引いて当てる。
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

  // このチャートが持つコンテナ名を登録し、画面全体で共通の色を引く。
  // 凡例 (MiniLegend) にも同じ colorOf を渡すので、線と凡例の色は必ず一致する。
  const containerNames = useMemo(
    () => datasets.flatMap((d) => (d.containerName ? [d.containerName] : [])),
    [datasets],
  );
  const colorOf = useContainerColors(containerNames);

  useEffect(() => {
    if (!refCanvas.current) return;

    refChart.current = new ChartJs<'line', TimedSeries>(refCanvas.current, {
      type: 'line',
      data: {
        datasets: datasets.map((d) => ({
          ...d,
          ...(d.containerName
            ? {
                borderColor: colorOf(d.containerName).border,
                backgroundColor: colorOf(d.containerName).background,
              }
            : {}),
          hidden: d.containerName ? hidden.has(d.containerName) : false,
        })),
      },
      options: {
        animation: false,
        maintainAspectRatio: false,
        // ホバー位置に最も近い 1 点だけ tooltip に出す (mode: 'nearest')。
        // intersect: false でカーソルが点の上になくてもヒットさせ、
        // hitRadius と合わせて当たり判定を拡張する。
        interaction: {
          mode: 'nearest',
          intersect: false,
        },
        plugins: {
          // タイトルは HTML 側で出してミニ凡例の上に置く。
          title: { display: false },
          legend: { display: false },
        },
        elements: {
          point: {
            radius: 0,        // 通常は非表示
            hoverRadius: 4,   // ホバー時のみ表示してフィードバック
            hitRadius: 10,    // 当たり判定だけ広げる (実描画より大きく)
          },
          line: { borderWidth: 2 },
        },
        scales: {
          x: {
            type: 'time',
            // データ範囲ではなく目盛境界 (= unit の startOf) に合わせて min/max を
            // 拡張する。auto 選択された unit (hour / minute) の境界に min が
            // 揃うので、24h レンジでは 0:00, 1:00,… のようにキリのいい時刻で
            // 目盛が切れる。
            bounds: 'ticks',
            time: {
              // unit を固定すると "データ開始分:秒 = startOf(minute) " 起点で
              // stepSize 分刻みの目盛になり、0:01, 0:04,… のような半端な
              // 時刻になる。unit は Chart.js に自動選択させる (24h なら hour,
              // 数十分なら minute)。
              minUnit: 'minute',
              displayFormats: {
                minute: 'HH:mm',
                hour: 'HH:mm',
              },
              tooltipFormat: 'yyyy-MM-dd HH:mm:ss',
            },
            ticks: {
              maxRotation: 0,
              autoSkip: true,
              autoSkipPadding: 20,
            },
          },
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
    // 動作優先で再生成を許容している。
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

  // 色の変更も、作り直さずに update('none') で反映する。
  // 画面全体の名前の一覧が変わると colorOf が新しくなり、全チャートでこれが走る。
  // (datasets が変わったときは上の effect が作り直した直後にここも走るが、同じ色を
  //  当て直すだけである)
  useEffect(() => {
    const chart = refChart.current;
    if (!chart) return;
    chart.data.datasets.forEach((d) => {
      if (!d.containerName) return;
      const color = colorOf(d.containerName);
      d.borderColor = color.border;
      d.backgroundColor = color.background;
    });
    chart.update('none');
  }, [colorOf, datasets]);

  return (
    <div className='w-full mb-2'>
      {title && (
        <h3 className='text-sm font-semibold px-2 pt-2'>{title}</h3>
      )}
      <MiniLegend datasets={datasets} colorOf={colorOf} />
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
