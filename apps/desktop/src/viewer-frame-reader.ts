import { invoke } from "@tauri-apps/api/core";

export type DeliveredFrame = {
  bytes: Uint8Array<ArrayBuffer>;
  delivery: "shared" | "ipc";
  release: () => void;
};
type SharedEvent = {
  additionalData?: { kind?: string; id?: string; request?: number };
  getBuffer: () => ArrayBuffer;
};
type Bridge = {
  addEventListener: (
    name: string,
    handler: (event: SharedEvent) => void,
  ) => void;
  releaseBuffer: (buffer: ArrayBuffer) => void;
};
type FrameCall = (
  command: string,
  args: Record<string, unknown>,
) => Promise<unknown>;
type Receiver = {
  next: number;
  pending: Map<
    number,
    { id: string; receive: (frame: DeliveredFrame) => void }
  >;
};
const receivers = new WeakMap<Bridge, Receiver>();
const empty = (): DeliveredFrame => ({
  bytes: new Uint8Array(0),
  delivery: "ipc",
  release() {},
});

function receiverFor(bridge: Bridge): Receiver {
  const existing = receivers.get(bridge);
  if (existing) return existing;
  const receiver: Receiver = { next: 0, pending: new Map() };
  bridge.addEventListener("sharedbufferreceived", (event) => {
    const data = event.additionalData;
    if (data?.kind !== "farsail-frame") return;
    const buffer = event.getBuffer();
    let released = false;
    const release = () => {
      if (!released) {
        released = true;
        bridge.releaseBuffer(buffer);
      }
    };
    const request = data.request;
    const pending =
      typeof request === "number" ? receiver.pending.get(request) : undefined;
    if (!pending || pending.id !== data.id) {
      release();
      return;
    }
    receiver.pending.delete(request!);
    pending.receive({
      bytes: new Uint8Array(buffer),
      delivery: "shared",
      release,
    });
  });
  receivers.set(bridge, receiver);
  return receiver;
}

/** Each viewer permits one request and owns its buffers until its JPEG is consumed. */
export class ViewerFrameReader {
  private bridge = (
    globalThis as typeof globalThis & { chrome?: { webview?: Bridge } }
  ).chrome?.webview;
  private shared = Boolean(
    this.bridge?.releaseBuffer && this.bridge?.addEventListener,
  );
  private generation = 0;
  private cancelPending: (() => void) | null = null;
  private buffers = new Set<DeliveredFrame>();
  private readonly call: FrameCall;
  constructor(call: FrameCall = invoke, bridge?: Bridge) {
    this.call = call;
    if (bridge) this.bridge = bridge;
    this.shared = Boolean(
      this.bridge?.releaseBuffer && this.bridge?.addEventListener,
    );
  }

  cancel() {
    this.generation++;
    this.cancelPending?.();
    this.cancelPending = null;
    for (const frame of this.buffers) frame.release();
    this.buffers.clear();
  }

  private async legacy(id: string, after: number): Promise<DeliveredFrame> {
    const response = (await this.call("media_next", {
      id,
      after,
    })) as ArrayBuffer;
    return { bytes: new Uint8Array(response), delivery: "ipc", release() {} };
  }

  async next(id: string, after: number): Promise<DeliveredFrame> {
    if (!this.shared || !this.bridge) return this.legacy(id, after);
    const generation = this.generation;
    const receiver = receiverFor(this.bridge);
    const request = ++receiver.next;
    if (!Number.isSafeInteger(request))
      throw new Error("画面请求编号已用尽，请重新打开窗口。");
    const delivered: { frame: DeliveredFrame | null } = { frame: null };
    let resolveFrame!: (frame: DeliveredFrame | null) => void;
    const received = new Promise<DeliveredFrame | null>((resolve) => {
      resolveFrame = resolve;
    });
    receiver.pending.set(request, {
      id,
      receive: (frame) => {
        const originalRelease = frame.release;
        frame.release = () => {
          originalRelease();
          this.buffers.delete(frame);
        };
        delivered.frame = frame;
        this.buffers.add(frame);
        resolveFrame(frame);
      },
    });
    const cancel = () => {
      receiver.pending.delete(request);
      resolveFrame(null);
    };
    this.cancelPending = cancel;
    let timer = 0;
    const deadline = new Promise<"timeout">((resolve) => {
      timer = window.setTimeout(() => resolve("timeout"), 2000);
    });
    try {
      const status = await Promise.race([
        this.call("media_next_shared", { id, after, request }),
        deadline,
      ]);
      if (generation !== this.generation) return empty();
      if (status === "shared") {
        const frame = await Promise.race([received, deadline.then(() => null)]);
        if (frame && generation === this.generation) return frame;
      }
      delivered.frame?.release();
      if (generation !== this.generation || status === "empty") return empty();
      this.shared = false;
      return this.legacy(id, after);
    } catch (error) {
      delivered.frame?.release();
      throw error;
    } finally {
      window.clearTimeout(timer);
      receiver.pending.delete(request);
      if (this.cancelPending === cancel) this.cancelPending = null;
    }
  }
}
