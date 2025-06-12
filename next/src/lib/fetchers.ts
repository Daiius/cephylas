
import z from 'zod';

const API_URL = process.env.API_URL!;

export type FetchResult<T> = 
  | { ok: true;  data: T; }
  | { ok: false; error: Error; }
  ;
const fetchWithZod = async <T>(
  url: string,
  schema: z.ZodSchema<T>
): Promise<FetchResult<T>> => {
  try {
    const response = await fetch(url);
    if (!response.ok) {
      console.error(response.statusText);
      return { 
        ok: false, 
        error: new Error(`fetch failed with ${response.status}`),
      };
    }
    const json = await response.json();
    const data = schema.parse(json);
    return { ok: true, data };
  } catch (err) {
    console.error(err);
    return {
      ok: false,
      error:
        err instanceof Error
        ? err
        : new Error('Unknown error')
    };
  }
}

// Containers
const containersSchema = z.array(z.string());
export const fetchContainers = async ()
  : Promise<FetchResult<z.infer<typeof containersSchema>>> => 
await fetchWithZod(`${API_URL}/containers`, containersSchema);

// CPU
const CpuUsageDataSchema = z.array(
  z.object({
    time: z.string().optional(), 
    percentage: z.number().optional().nullish(),
  })
);
const CpuUsageDatasetSchema = CpuUsageDataSchema.transform((data) => 
  data.map(d => ({ x: d.time, y: d.percentage }))
);

export type CpuUsageDatasets = {
  label: string;
  data: z.infer<typeof CpuUsageDatasetSchema>;
}[];

export const fetchCpuStatus = async (
  containerName: string
): Promise<FetchResult<z.infer<typeof CpuUsageDatasetSchema>>> => { 
  const result = await fetchWithZod(
    `${API_URL}/containers/${containerName}/cpu`, 
    CpuUsageDataSchema,
  );
  return result.ok
    ? { ok: result.ok, data: result.data.map(d => ({ x: d.time, y: d.percentage })) }
    : result;
}

// Memory
const MemoryUsageDataSchema = z.array(
  z.object({
    time: z.string().optional(), 
    percentage: z.number().optional().nullish(),
  })
);
const MemoryUsageDatasetSchema = MemoryUsageDataSchema.transform((data) => 
  data.map(d => ({ x: d.time, y: d.percentage }))
);
export type MemoryUsageDatasets = {
  label: string;
  data: z.infer<typeof MemoryUsageDatasetSchema>;
}[];

export const fetchMemoryStatus = async (
  containerName: string
): Promise<FetchResult<z.infer<typeof MemoryUsageDatasetSchema>>> => { 
  const result = await fetchWithZod(
    `${API_URL}/containers/${containerName}/memory`, 
    CpuUsageDataSchema,
  );
  return result.ok
    ? { ok: result.ok, data: result.data.map(d => ({ x: d.time, y: d.percentage })) }
    : result;
}

// IO
const IoUsageDataSchema = z.array(
  z.object({
    time: z.string().optional(),
    readkBps: z.number().nullish(),
    writekBps: z.number().nullish(),
  })
);
const IoReadDatasetSchema = IoUsageDataSchema.transform((data) => 
  data.map(d => ({ x: d.time, y: d.readkBps }))
);
const IoWriteDatasetSchema = IoUsageDataSchema.transform((data) =>
 data.map(d => ({ x: d.time, y: d.writekBps }))
);

export type IoUsageDatasets = {
  label: string;
  data: z.infer<typeof IoReadDatasetSchema>;
  backgroundColor: string;
  borderColor: string;
  borderDash: [number, number];
}[];

export const fetchIoStatus = async (
  containerName: string,
  type: 'read' | 'write',
): Promise<
  FetchResult<
      z.infer<typeof IoReadDatasetSchema>
    | z.infer<typeof IoWriteDatasetSchema>
  >
> => {
  const response = await fetchWithZod(
    `${API_URL}/containers/${containerName}/io/${type}`,
    IoUsageDataSchema,
  );
  return response.ok
    ? type === 'read'
      ? { ...response, data: response.data.map(d => ({ x: d.time, y: d.readkBps })) }
      : { ...response, data: response.data.map(d => ({ x: d.time, y: d.writekBps })) }
    : response;
}

// Net
const NetUsageDataSchema = z.array(
  z.object({
    time: z.string().nullish(),
    recvkBps: z.number().nullish(),
    sendkBps: z.number().nullish(),
  })
);
const NetRecvDatasetSchema = NetUsageDataSchema.transform((data) => 
  data.map(d => ({ x: d.time, y: d.recvkBps }))
);
const NetSendDatasetSchema = NetUsageDataSchema.transform((data) =>
 data.map(d => ({ x: d.time, y: d.sendkBps }))
);

export type NetUsageDatasets = {
  label: string;
  data: z.infer<typeof NetRecvDatasetSchema>;
  backgroundColor: string;
  borderColor: string;
  borderDash: [number, number];
}[];

export const fetchNetStatus = async (
  containerName: string,
  type: 'recv' | 'send',
): Promise<
  FetchResult<
      z.infer<typeof NetRecvDatasetSchema>
    | z.infer<typeof NetSendDatasetSchema>
  >
> => {
  const response = await fetchWithZod(
    `${API_URL}/containers/${containerName}/net/${type}`,
    NetUsageDataSchema,
  );
  return response.ok
    ? type === 'recv'
      ? { ...response, data: response.data.map(d => ({ x: d.time, y: d.recvkBps })) }
      : { ...response, data: response.data.map(d => ({ x: d.time, y: d.sendkBps })) }
    : response;
}

