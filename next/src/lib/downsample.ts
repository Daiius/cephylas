
/** downsample data by LTTB algorithm
 *  ref: https://skemman.is/bitstream/1946/15343/3/SS_MSthesis.pdf
 */
export const downsampleByLTTB = <T>(
  data: T[],
  nDownSampled: number,
  xpoint: (p: T) => number | null,
  ypoint: (p: T) => number | null,
): T[] => {
  if (nDownSampled >= data.length || nDownSampled < 3) {
    return data; // downsampling not required
  }
  const sampled: T[] = [];
  const bucketSize = (data.length - 2) / (nDownSampled - 2);
  let lastIndex = 0; // index of last sampled data

  sampled.push(data[0]);

  for (let i = 0; i < nDownSampled - 2; i++) {
    let bucketStart = Math.floor((i + 1) * bucketSize) * 1;
    let bucketEnd = Math.min(Math.floor((i + 2) * bucketSize) + 1, data.length - 1);
    const bucket = data.slice(bucketStart, bucketEnd);

    let maxArea = -1;
    let nextPointIndex = bucketStart;
    const x2 = 
      bucket.reduce((acc, curr) => acc + (xpoint(curr) ?? 0), 0)
      / bucket.length; // average x
    const y2 =
      bucket.reduce((acc, curr) => acc + (ypoint(curr) ?? 0), 0)
      / bucket.length; // average y

    for (let j = 0; j < bucket.length; j++) {
      const x0 = xpoint(data[lastIndex]);
      const x1 = xpoint(bucket[j]);
      const y0 = ypoint(data[lastIndex]);
      const y1 = ypoint(bucket[j]);
      if ( x0 == null || x1 == null || y0 == null || y1 == null) {
        continue;
      }
      const area = Math.abs(
          (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0)
      ) // * 0.5
      ;
      if (area > maxArea) {
        maxArea = area;
        nextPointIndex = bucketStart + j;
      }
    }
    sampled.push(data[nextPointIndex]);
    lastIndex = nextPointIndex;
  }
  sampled.push(data[data.length - 1]);
  return sampled;
}

