import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  basePath: "/cephylas",
  publicRuntimeConfig: {
    basePath: "/cephylas",
  },
  // for docker container dev environment
  watchOptions: { pollIntervalMs: 1000, },
  webpack: (config) => ({
    ...config,
    watchOptions: { poll: 1000, },
  }),
};

export default nextConfig;

