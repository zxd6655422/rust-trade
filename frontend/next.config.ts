const nextConfig = {
  // output: 'export',  // 开发时注释掉，构建时取消注释
  images: {
    unoptimized: true,
  },
  // assetPrefix: './',
  trailingSlash: true,
  webpack: (config: { resolve: { fallback: any; }; }) => {
    config.resolve.fallback = {
      ...(config.resolve.fallback || {}),
      fs: false,
      path: false,
    };
    return config;
  },
};

module.exports = nextConfig;
