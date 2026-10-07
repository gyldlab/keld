// Shared layout of the arm B SharedArrayBuffer: [CTRL words][event ring][reply slot].
export const CTRL_BYTES = 128;
export const CTRL = {
  SEQ: 0, // bumped by the worker on every reply / close / overflow; main waits here
  STATE: 1,
  W: 2, // monotonic ring write byte counter (prototype: < 2^31 per session)
  R: 3, // monotonic ring read byte counter, advanced by main only
  EVENTS: 4,
  REPLY_KIND: 5,
  REPLY_CHANNEL: 6,
  REPLY_CORR: 7,
  REPLY_LEN: 8,
  CLOSE_CODE: 9,
  HIGH_WATER: 10,
} as const;
export const STATE = { OPEN: 0, CLOSED: 1, OVERFLOW: 2 } as const;
export const CLOSE = { PEER: 1, OVERFLOW: 2 } as const;
