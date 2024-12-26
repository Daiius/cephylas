
import { readFile } from 'fs/promises';
import { createReadStream } from 'fs';
import { createInterface } from 'readline';
import { z } from 'zod';

const LOG_PATH = "/app/log/log_daily";

const logTimeSchema = z.preprocess(
  (arg) => {
    if (typeof arg === "string") {
      const formatted = arg.replace(/[\s/\.]/g, "T");
      const date = new Date(formatted);
      return isNaN(date.getTime()) ? undefined : date;
    }
  },
  z.date()
);
const statsTimeSchema = z.preprocess(
  (arg) =>
    typeof arg === "string"
      ? new Date(arg)
      : arg,
  z.date()
);

const statsSchema = z.object({
  time: statsTimeSchema,
  cpu: z.object({
    total: z.number().nullable(),
    system: z.number().nullable(),
    ncpu: z.number().nullable(),
  }),
  memory: z.object({
    used: z.number().nullable(),
    available: z.number().nullable(),
  }),
  io: z.object({
    read: z.number().nullable(),
    write: z.number().nullable(),
  }),
  net: z.object({
    send: z.number().nullable(),
    recv: z.number().nullable(),
  }),
});

const logLineSchema = z.object({
  time: logTimeSchema,
  stats: z.record(z.string(), statsSchema),
});

type Log = z.infer<typeof logLineSchema>;

type LogDiff = Log & { millis: number };


type ResourceUsage = {
  millis: number,
  time: Date,
  cpu: {
    percentage: number|null,
    delta: number|null,
    systemDelta: number|null,
    nCpu: number|null,
  },
  memory: {
    percentage: number|null,
    availableBytes: number|null,
    usedBytes: number|null,
  },
  io: {
    readBytes: number|null,
    writtenBytes: number|null,
    readBytesPerSecs: number|null,
    writtenBytesPerSecs: number|null,
  },
  net: {
    sendMBytes: number|null,
    sendMbps: number|null,
    recvMBytes: number|null,
    recvMbps: number|null,
  }
}

    

const nullableSub = (
  a: number | null, 
  b: number | null
): number | null => (
  a === null || b === null
    ? null
    : a - b
);

const diffLog = (a: Log, b: Log): LogDiff => ({
  time: 
    new Date(b.time as unknown as string),
  millis: 
    new Date(b.time as unknown as string).getTime() 
    - new Date(a.time as unknown as string).getTime(),
  stats: 
    Object.entries(b.stats)
      .map(([key, bvalue]) => {
        const prev_stat = a.stats[key as keyof(typeof a.stats)];
        if (prev_stat == null) return {};
        return {
          [key as keyof (typeof a.stats)]: {
          ...bvalue,
          cpu: {
            ...bvalue.cpu,
            total: 
              bvalue.cpu.total == null || prev_stat.cpu.total == null 
                ? null 
                : bvalue.cpu.total - prev_stat.cpu.total,
            system:
              bvalue.cpu.system == null || prev_stat.cpu.system == null
                ? null
                : bvalue.cpu.system - prev_stat.cpu.system,
          },
          io: {
            read:
              bvalue.io.read == null || prev_stat.io.read == null
                ? null
                : bvalue.io.read - prev_stat.io.read,
            write:
              bvalue.io.write == null || prev_stat.io.write == null
                ? null
                : bvalue.io.write - prev_stat.io.write,
          },
          net: {
            send:
              nullableSub(
                bvalue.net.send,
                prev_stat.net.send
              ),
            recv:
              nullableSub(
                bvalue.net.recv,
                prev_stat.net.recv
              ),
          }
        }};
      })
      .reduce((acc, curr) => ({ ...acc, ...curr }), {}),
});


// 現在は [{[container_name]: stats}] の様になっているが、
// {[container_name]: stats[]} の方が扱いやすい

const logDiffsToUsages = (
  diffs: LogDiff[]
): Record<string, ResourceUsage[]> => {
  const containerNames = [
    ...new Set(diffs.flatMap(d => Object.keys(d.stats)))
  ];
  const usages: Record<string, ResourceUsage[]> = {};
  for (const containerName of containerNames) {
    usages[containerName] = diffs.map(d => {
      const stat = d.stats[containerName];
      if (stat == null) return undefined;
      const usage: ResourceUsage = {
        time: d.time,
        millis: d.millis,
        cpu: {
          percentage:
            stat.cpu.total == null || stat.cpu.system == null || stat.cpu.ncpu == null
              ? null 
              : stat.cpu.total / stat.cpu.system * stat.cpu.ncpu * 100.0,
          delta: stat.cpu.total,
          systemDelta: stat.cpu.system,
          nCpu: stat.cpu.ncpu,
        },
        memory: {
          percentage:
            stat.memory.used == null || stat.memory.available == null
              ? null
              : stat.memory.used / stat.memory.available * 100.0,
          availableBytes: stat.memory.available,
          usedBytes: stat.memory.used,
        },
        net: {
          recvMBytes: 
            stat.net.recv == null 
              ? null
              : stat.net.recv / 1_000_000,
          recvMbps:
            stat.net.recv == null
              ? null
              : (stat.net.recv * 8) / 1_000_000 / (d.millis / 1_000),
          sendMBytes:
            stat.net.send == null
              ? null
              : stat.net.send / 1_000_000,
          sendMbps:
            stat.net.send == null
              ? null
              : (stat.net.send * 8) / 1_000_000 / (d.millis / 1_000),
        },
        io: {
          readBytes: stat.io.read,
          readBytesPerSecs: 
            stat.io.read == null
              ? null
              : stat.io.read / (d.millis / 1_000),
          writtenBytes: stat.io.write,
          writtenBytesPerSecs: 
            stat.io.write == null
              ? null
              : stat.io.write / (d.millis / 1_000),
        }
      };
      return usage;
    })
    .filter(u => u != null) as ResourceUsage[];
  }
  return usages;
}

export const readLogs = async (): Promise<
  Record<string, ResourceUsage[]>
> => {

  var logs: Log[] = [];
  try {

    const nlines = 
      (await readFile(LOG_PATH, { encoding: 'utf-8', flag: 'r' }))
      .split('\n')
      .length;
    const nlinesToSkip = Math.max(nlines - 6 * 60 * 24, 0);
    
    console.time("ストリーム読み込み+JSONパース");
    const stream = createReadStream(LOG_PATH, { encoding: 'utf-8'});
    const reader = createInterface({ input: stream });

    var iline = 0;
    for await (const line of reader) {
      iline++;
      if (!line.trim()) continue;
      if (iline < nlinesToSkip) continue;
      const parsedLine = //logLineSchema.parse(
        JSON.parse(line.trim());
      //);
      logs.push(parsedLine);
      // 暫定1日分(10秒ごと1行)を保持
      //if (logs.length > 6 * 60 * 24) logs.shift();
    }
  } catch (err) {
    console.log("failed to parse log file.", err);
  } finally {
    console.timeEnd("ストリーム読み込み+JSONパース");
  }

  var diffs: LogDiff[] = [];
  try {
    console.time("差分計算");
    diffs = logs
      .slice(1)
      .map((value, index) => diffLog(logs[index], value));
  } catch (err) {
    console.log("failed to calc diff of log lines.", err);
  } finally {
    console.timeEnd("差分計算");
  }

  var usages: Record<string, ResourceUsage[]> = {};
  try {
    console.time("使用率計算");
    usages = logDiffsToUsages(diffs);
  } catch (err) {
    console.log("failed to calc resource usages.", err);
  } finally {
    console.timeEnd("使用率計算");
  }

  return usages;
};

