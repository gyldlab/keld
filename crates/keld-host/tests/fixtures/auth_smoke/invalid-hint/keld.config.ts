export default {
  name: "Keld Auth Smoke",
  entry: "src/main.ts",
  renderer: "index.html",
  auth: {
    "entra-common": { clientId: "keld-smoke-placeholder-no-registration" },
  },
} as const;
