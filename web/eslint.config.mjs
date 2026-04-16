import nextConfig from "eslint-config-next";

export default [
  ...nextConfig,
  {
    rules: {
      // Static export — next/image optimization is unavailable at runtime
      "@next/next/no-img-element": "off",
    },
  },
];
