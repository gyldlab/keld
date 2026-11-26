/**
 * Review-only transport allocation model. One instance belongs to one
 * existing authenticated link/generation. No frames, new wire values or
 * production WorkerLink changes. The owner supplies independent quiescence
 * evidence; a timeout/abandon is not that evidence.
 */
export type AttemptLease = Readonly<{ identity: symbol }>;
export type CorrelationLease = Readonly<{
  correlation: number;
  kind: "effect" | "status" | "ordinary";
}>;
type Entry = {
  lease: CorrelationLease;
  consumed: boolean;
  noLate: boolean;
};

export class DraftCorrelationLedger {
  readonly #max: number;
  #next: number;
  readonly #entries = new Map<number, Entry>();
  #active?: AttemptLease;
  #original?: CorrelationLease;
  #status?: CorrelationLease;

  // Small bounds/starting position are fixture-only exhaustion controls.
  constructor(max = 0xffff_ffff, next = 1) {
    if (!Number.isInteger(max) || max < 1 || max > 0xffff_ffff ||
        !Number.isInteger(next) || next < 1 || next > max)
      throw new RangeError("invalid draft allocator bounds");
    this.#max = max;
    this.#next = next;
  }

  get reservedCount(): number { return this.#entries.size; }
  get active(): boolean { return this.#active !== undefined; }
  get original(): CorrelationLease | undefined { return this.#original; }
  get status(): CorrelationLease | undefined { return this.#status; }

  beginAttempt(): AttemptLease | undefined {
    if (this.#active) return undefined;
    const attempt = Object.freeze({ identity: Symbol("draft attempt") });
    this.#active = attempt;
    return attempt;
  }

  endAttempt(attempt: AttemptLease): boolean {
    if (attempt !== this.#active || this.#original || this.#status) return false;
    this.#active = undefined;
    return true;
  }

  #allocate(kind: CorrelationLease["kind"]): CorrelationLease | undefined {
    if (this.#entries.size >= this.#max) return undefined;
    // With N occupied slots at most N+1 probes find a free one. Never spin
    // over the entire u32 domain or evict an unresolved entry for capacity.
    for (let probes = 0; probes <= this.#entries.size; probes++) {
      const correlation = this.#next;
      this.#next = correlation === this.#max ? 1 : correlation + 1;
      if (this.#entries.has(correlation)) continue;
      const lease = Object.freeze({ correlation, kind });
      this.#entries.set(correlation, { lease, consumed: false, noLate: false });
      return lease;
    }
    return undefined;
  }

  reserveOrdinary(): CorrelationLease | undefined {
    return this.#allocate("ordinary");
  }

  reserveEffect(attempt: AttemptLease): CorrelationLease | undefined {
    if (attempt !== this.#active || this.#original || this.#status) return undefined;
    const lease = this.#allocate("effect");
    if (lease) this.#original = lease;
    return lease;
  }

  reserveStatus(attempt: AttemptLease, original: CorrelationLease): CorrelationLease | undefined {
    if (attempt !== this.#active || original !== this.#original || this.#status)
      return undefined;
    const lease = this.#allocate("status");
    if (lease) this.#status = lease;
    return lease;
  }

  #entry(lease: CorrelationLease): Entry | undefined {
    const entry = this.#entries.get(lease.correlation);
    // Equality by object identity, not by recycled numeric correlation.
    return entry?.lease === lease ? entry : undefined;
  }

  isReserved(lease: CorrelationLease): boolean {
    return this.#entry(lease) !== undefined;
  }

  #release(entry: Entry): boolean {
    if (!entry.consumed || !entry.noLate) return false;
    this.#entries.delete(entry.lease.correlation);
    if (this.#original === entry.lease) this.#original = undefined;
    if (this.#status === entry.lease) this.#status = undefined;
    return true;
  }

  /** API has explicitly consumed the exact receipt; does not imply quiescence. */
  consume(lease: CorrelationLease): boolean {
    const entry = this.#entry(lease);
    if (!entry) return false;
    entry.consumed = true;
    return this.#release(entry);
  }

  /** Independent transport proof: this identity can never deliver a late reply. */
  proveNoLate(lease: CorrelationLease): boolean {
    const entry = this.#entry(lease);
    if (!entry) return false;
    entry.noLate = true;
    return this.#release(entry);
  }
}
