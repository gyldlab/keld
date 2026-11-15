/** Public filesystem calls over the application's single canonical link. */
import { kipcError } from "../../kipc/src/transport.ts";
import { invokeFs } from "./app.ts";
import { decodeFsResponse, encodeFsRequest } from "./fs.generated.ts";

/** Filesystem authority and native bounds remain enforced by the host. */
export const fs: {
  read(path: string): Promise<Uint8Array>;
  write(path: string, bytes: Uint8Array): Promise<void>;
} = {
  async read(path: string): Promise<Uint8Array> {
    // Encoding occurs before the first await; local failures reject this Promise.
    const payload = encodeFsRequest({ variant: "Read", path });
    const response = decodeFsResponse(await invokeFs(payload));
    if (response.variant !== "Read") {
      throw kipcError("KELD-IPC-003", "fs.read received a Write response");
    }
    return response.bytes;
  },

  async write(path: string, bytes: Uint8Array): Promise<void> {
    // The generated encoder owns the caller's bytes before readiness can yield.
    const payload = encodeFsRequest({ variant: "Write", path, bytes });
    const response = decodeFsResponse(await invokeFs(payload));
    if (response.variant !== "Write") {
      throw kipcError("KELD-IPC-003", "fs.write received a Read response");
    }
  },
};
