// The native window owns tab membership. User mutations are ordered, while
// snapshots are single-flight and may only publish in the intent/lifetime that
// started them. Keeping this controller across effect replays also keeps the
// old in-flight read from overlapping the next one.
export class ViewerTabsController<T> {
  private active = false;
  private lifetime = 0;
  private revision = 0;
  private pending = 0;
  private requested = false;
  private reading: Promise<void> | null = null;
  private mutations: Promise<void> = Promise.resolve();
  private readonly read: () => Promise<unknown>;
  private readonly publish: (tabs: T[]) => void;
  private readonly failed: (error: unknown) => void;

  constructor(options: {
    read: () => Promise<unknown>;
    publish: (tabs: T[]) => void;
    failed: (error: unknown) => void;
  }) {
    this.read = options.read;
    this.publish = options.publish;
    this.failed = options.failed;
  }

  start() {
    this.active = true;
    this.refresh();
  }

  stop() {
    this.active = false;
    this.lifetime++;
    this.requested = false;
  }

  refresh() {
    if (!this.active) return;
    this.requested = true;
    this.drain();
  }

  mutate(
    action: () => Promise<unknown>,
    reportFailure: "latest" | "always" = "latest",
  ) {
    if (!this.active) return Promise.resolve();
    const lifetime = this.lifetime;
    const revision = ++this.revision;
    this.pending++;
    // A previously started read is now stale. A new read is required even if
    // this mutation finishes before that stale read returns.
    this.requested = true;
    this.mutations = this.mutations.then(async () => {
      try {
        if (!this.active || lifetime !== this.lifetime) return;
        await action();
      } catch (error) {
        if (
          this.active &&
          lifetime === this.lifetime &&
          (reportFailure === "always" || revision === this.revision)
        )
          this.failed(error);
      } finally {
        this.pending--;
        this.drain();
      }
    });
    return this.mutations;
  }

  private drain() {
    if (!this.active || this.pending || this.reading || !this.requested) return;
    this.reading = this.readSnapshots().finally(() => {
      this.reading = null;
      this.drain();
    });
  }

  private async readSnapshots() {
    while (this.active && !this.pending && this.requested) {
      this.requested = false;
      const lifetime = this.lifetime;
      const revision = this.revision;
      const current = () =>
        this.active &&
        lifetime === this.lifetime &&
        revision === this.revision &&
        !this.pending;
      try {
        const tabs = await this.read();
        if (current() && Array.isArray(tabs)) this.publish(tabs as T[]);
      } catch (error) {
        if (current()) this.failed(error);
      }
    }
  }
}
