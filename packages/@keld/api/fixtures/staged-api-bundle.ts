const [entry, out] = process.argv.slice(2);
const result = await Bun.build({
  entrypoints: [entry],
  target: "bun",
  format: "esm",
  plugins: [
    {
      name: "kipc-transport-sidecar",
      setup(build) {
        build.onResolve(
          { filter: /^(?:@keld\/kipc)$|[\\/]kipc[\\/]src[\\/]transport\.ts$/ },
          () => ({
            path: "./kipc-transport.ts",
            external: true,
          }),
        );
      },
    },
  ],
});
if (!result.success) {
  console.error(result.logs);
  process.exit(1);
}
await Bun.write(out, result.outputs[0]);
