import React from 'react';
import clsx from 'clsx';

export const ChartSkeleton: React.FC = () => (
  <div className={clsx(
    'w-full h-[80vh] rounded-lg shadow-xl',
    'bg-slate-200',
  )}>
  </div>
);

export default ChartSkeleton;

