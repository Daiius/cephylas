import http from 'k6/http';
import { check, sleep } from 'k6';
import { Options } from 'k6/options';

export const options: Options = {
  vus: 10,
  duration: '30s',
};

const BASE_URL = __ENV.API_URL || 'http://localhost:7878';

export default function (): void {
  const containers = http.get(`${BASE_URL}/containers`);
  check(containers, { 'containers status 200': (r) => r.status === 200 });

  try {
    const containerList: string[] = JSON.parse(containers.body as string);
    for (const name of containerList.slice(0, 3)) {
      http.get(`${BASE_URL}/containers/${name}/cpu`);
      http.get(`${BASE_URL}/containers/${name}/memory`);
    }
  } catch {
    // Skip if response is not valid JSON
  }

  sleep(0.1);
}
