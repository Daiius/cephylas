// Chart.js の型を declaration merging で拡張する。
// dataset にコンテナ名を持たせて FilterContext と紐付けるため。
// ref: https://www.chartjs.org/docs/latest/general/options.html#dataset-level-options
import 'chart.js';

declare module 'chart.js' {
  interface ChartDatasetProperties<TType extends ChartType, TData> {
    /** どのコンテナの dataset か。Sidebar の hidden 判定に使う */
    containerName?: string;
  }
}
