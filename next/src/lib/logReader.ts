
import { createReadStream } from 'fs';
import { createInterface } from 'readline';
import { z } from 'zod';

const LOG_PATH = "/app/log/log_daily";

const timeSchema = z.preprocess(
  (arg) =>
    typeof arg === "string"
      ? new Date(arg)
      : arg,
  z.date()
);

const StatsSchema = z.object({
  cpu: z.object({
    percentage: z.number().nullable(),
    total: z.number().nullable(),
    system: z.number().nullable(),
  }),
  memory: z.object({
    percentage: z.number().nullable(),
    used: z.number().nullable(),
    available: z.number().nullable(),
  }),
  io: z.object({
    readkBps: z.number().nullable(),
    writekBps: z.number().nullable(),
    readkB: z.number().nullable(),
    writekB: z.number().nullable(),
  }),
  net: z.object({
    sendkBps: z.number().nullable(),
    recvkBps: z.number().nullable(),
    sendkB: z.number().nullable(),
    recvkB: z.number().nullable(),
  }),
});

const LogSchema = z.object({
  time: timeSchema,
  millis: z.number(),
  stats: z.record(z.string(), StatsSchema),
});

type Log = z.infer<typeof LogSchema>;
type Stat = z.infer<typeof StatsSchema>;


export const readLogs = async (): Promise<
  Record<string, (Stat & { time: Date })[]>
> => {
  var logs: Record<string, (Stat & { time: Date })[]> = {};
  try {
    console.time("ストリーム読み込み+JSONパース");
    const stream = createReadStream(LOG_PATH, { encoding: 'utf-8'});
    const reader = createInterface({ input: stream });

    //for (const line of (await readFile(LOG_PATH, { flag: 'r' })).toString('utf-8').split('\n')) {

    for await (const line of reader) {
    //for (const line of await readLastLines(LOG_PATH, 8000, 131072)) {
      const trimmedLine = line.trim();
      if (!trimmedLine) continue;
      const parsedLog: Log = LogSchema.parse(
        JSON.parse(trimmedLine)
      )
      ;
      const containerNames = Object.keys(parsedLog.stats);
      for (const containerName of containerNames) {
        if (logs[containerName] == null) {
          logs[containerName] = [];
        }
        const parsedStat: Stat = parsedLog.stats[containerName];
        logs[containerName]?.push({
          time: parsedLog.time,
          ...parsedStat
        });
      }
    }

  } catch (err) {
    console.log("failed to parse log file.", err);
  } finally {
    console.timeEnd("ストリーム読み込み+JSONパース");
   
  }

  return logs;
};

