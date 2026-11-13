// Explicit unqualified inputs are expected to fail before packaging.
export default {
  target: "macos-arm64",
  signing: "adhoc-local",
  bundleVersion: "1",
  minimumSystemVersion: "14.0",
  resources: ["src", "web"],
  host: {
    path: "/UNQUALIFIED_GH1404_HOST",
    sha256: "UNQUALIFIED_HOST_SHA256",
    version: "unqualified",
  },
  bun: {
    path: "/UNQUALIFIED_GH1404_BUN",
    sha256: "UNQUALIFIED_BUN_SHA256",
    version: "unqualified",
  },
  graphSource: {
    packageSha256: "bb9e988bc67c7ca000f2ada2d5d9043903b31db54d46f2ff6487a575ae712a55",
    lockSha256: "dc3a2fa177d9f416bcfc45e663d94d20121282208dbd2b681a797f7314b81652",
    providerTrees: {
      electron: "UNQUALIFIED_PROVIDER_ELECTRON",
      api: "UNQUALIFIED_PROVIDER_API",
      kipc: "UNQUALIFIED_PROVIDER_KIPC",
    },
    preload: {
      path: "src/preload.mjs",
      sha256: "597854316c80b4da28f891d51d8d4ab18e0ff7d918d417d69b5a6e5f659b37b8",
    },
  },
  outDir: "dist",
} as const;
