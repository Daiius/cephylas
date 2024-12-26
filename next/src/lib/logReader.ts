
import { readFile } from 'fs/promises';
import { z } from 'zod';

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
    total: z.number(),
    system: z.number(),
    ncpu: z.number(),
  }),
  memory: z.object({
    used: z.number().nullable(),
    available: z.number(),
  }),
  io: z.object({
    read: z.number(),
    write: z.number(),
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
    percentage: number,
    delta: number,
    systemDelta: number,
    nCpu: number,
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
  time: b.time,
  millis: b.time.getTime() - a.time.getTime(),
  stats: 
    Object.entries(b.stats)
      .map(([key, bvalue]) => ({
        [key as keyof (typeof a.stats)]: {
          ...bvalue,
          cpu: {
            ...bvalue.cpu,
            total: 
              bvalue.cpu.total 
              - a.stats[key as keyof (typeof a.stats)].cpu.total,
            system:
              bvalue.cpu.system
              - a.stats[key as keyof (typeof a.stats)].cpu.system,
          },
          io: {
            read:
              bvalue.io.read
              - a.stats[key as keyof(typeof a.stats)].io.read,
            write:
              bvalue.io.write
              - a.stats[key as keyof(typeof a.stats)].io.write,
          },
          net: {
            send:
              nullableSub(
                bvalue.net.send,
                a.stats[key as keyof(typeof a.stats)].net.send
              ),
            recv:
              nullableSub(
                bvalue.net.recv,
                a.stats[key as keyof(typeof a.stats)].net.recv
              ),
          }
        }
      }))
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
      const usage: ResourceUsage = {
        time: d.time,
        millis: d.millis,
        cpu: {
          percentage: 
            stat.cpu.total / stat.cpu.system * stat.cpu.ncpu * 100.0,
          delta: stat.cpu.total,
          systemDelta: stat.cpu.system,
          nCpu: stat.cpu.ncpu,
        },
        memory: {
          percentage:
            stat.memory.used == null
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
          readBytesPerSecs: stat.io.read / (d.millis / 1_000),
          writtenBytes: stat.io.write,
          writtenBytesPerSecs: stat.io.write / (d.millis / 1_000),
        }
      };
      return usage;
    });
  }
  return usages;
}

export const readLogs = async (): Promise<
  Record<string, ResourceUsage[]>
> => {
  const rawData = await readFile(
    "/app/log/log_daily",
    { encoding: 'utf-8', flag: 'r'}
  );
  const logs: Log[] = rawData
    .split("\n")
    .map(l => l.trim())
    .filter(l => l)
    .map(l => logLineSchema.parse(JSON.parse(l)))

  const diffs = logs
    .slice(1)
    .map((value, index) => diffLog(logs[index], value));

  const usages = logDiffsToUsages(diffs);
  //console.log(usages);

  return usages;
};
