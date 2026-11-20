/** @type {import('next').NextConfig} */
import path from "node:path";

const nextConfig = {
  reactStrictMode: true,
  transpilePackages: ["@soroban-forge/escrow-client"],
  turbopack: {
    root: path.resolve(process.cwd(), ".."),
  },
}

export default nextConfig
