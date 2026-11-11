import assert from "node:assert/strict";
import { decodeInputs, encodeOutputs } from "./protocol.ts";
function request(renderer = "<p>hello</p>") {
  const chunks = [Buffer.from("KELDCIN3")];
  const u32 = (n) => {
    const b = Buffer.alloc(4);
    b.writeUInt32LE(n);
    chunks.push(b);
  };
  const part = (s) => {
    const b = Buffer.from(s);
    u32(b.length);
    chunks.push(b);
  };
  part("src/main.ts");
  u32(1);
  part("src/main.ts");
  part("export const x=1;");
  part(renderer);
  return Buffer.concat(chunks);
}
const frame = request();
const decoded = decodeInputs(frame);
assert.equal(Buffer.from(decoded.renderer).toString(), "<p>hello</p>");
assert.equal(decoded.modules.size, 1);
assert.equal(decoded.entry, "src/main.ts");
for (let i = 0; i < frame.length; i++)
  assert.throws(() => decodeInputs(frame.subarray(0, i)));
assert.throws(() => decodeInputs(Buffer.concat([frame, Buffer.from([0])])));
assert.throws(() => decodeInputs(request("")));
const digest = new Uint8Array(32).fill(7),
  main = Buffer.from("main"),
  transport = Buffer.from("transport");
const output = encodeOutputs(main, transport, digest);
assert.equal(output.length, 48 + main.length + transport.length);
assert.deepEqual(output.subarray(8, 40), digest);
assert.equal(new DataView(output.buffer).getUint32(40, true), 4);
assert.equal(Buffer.from(output.subarray(44, 48)).toString(), "main");
assert.equal(new DataView(output.buffer).getUint32(48, true), 9);
assert.equal(Buffer.from(output.subarray(52)).toString(), "transport");
assert.throws(() => encodeOutputs(main, transport, new Uint8Array(31)));
console.log(
  "v3 declared-entry renderer-bound protocol tests passed (not compiler/native evidence)",
);

const absent = Buffer.from(frame);
absent.write("src/nope.ts", 12);
assert.throws(() => decodeInputs(absent), /Declared entry absent/);
const emptyEntry = Buffer.from(frame);
emptyEntry.writeUInt32LE(0, 8);
assert.throws(() => decodeInputs(emptyEntry), /entry bound/);
const oldVersion = Buffer.from(frame);
oldVersion.write("KELDCIN2", 0);
assert.throws(() => decodeInputs(oldVersion), /version/);
