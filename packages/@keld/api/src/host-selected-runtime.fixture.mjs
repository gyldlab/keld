import { bootHostSelectedEntry } from "./app.ts";
await bootHostSelectedEntry(() => import("./runtime-preload-entry.fixture.mjs"));
