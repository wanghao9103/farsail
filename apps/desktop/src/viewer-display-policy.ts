export type RemoteDisplay = {
  id: number;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  dpi: number;
  rotation: number;
};

type DisplayStage =
  | "ready"
  | "waiting_list"
  | "unavailable"
  | "selecting"
  | "awaiting_frame"
  | "stopped";
export type DisplayTicket = { epoch: number; display: number };
export type DisplaySnapshot = {
  stage: DisplayStage;
  target: number | null;
  epoch: number;
  issue: "missing" | "ambiguous" | "request" | "timeout" | null;
};

// Friendly names and enumeration IDs are not hardware identities. Only a
// unique name/desktop rectangle match may restore a choice automatically.
export function matchRetainedDisplay(
  choice: RemoteDisplay,
  list: RemoteDisplay[],
) {
  const matches = list.filter(
    (d) =>
      d.name === choice.name &&
      d.x === choice.x &&
      d.y === choice.y &&
      d.width === choice.width &&
      d.height === choice.height,
  );
  return matches.length === 1
    ? { kind: "matched" as const, display: matches[0] }
    : { kind: matches.length ? ("ambiguous" as const) : ("missing" as const) };
}

export class ViewerDisplayGate {
  private stage: DisplayStage;
  private choice: RemoteDisplay | null;
  private target: number | null = null;
  private epoch = 0;
  private issue: DisplaySnapshot["issue"] = null;

  constructor(choice: RemoteDisplay | null, restoring: boolean) {
    this.choice = choice ? { ...choice } : null;
    this.stage = restoring ? "waiting_list" : "ready";
  }
  snapshot(): DisplaySnapshot {
    return {
      stage: this.stage,
      target: this.target,
      epoch: this.epoch,
      issue: this.issue,
    };
  }
  intent() {
    return this.choice ? { ...this.choice } : null;
  }
  canControl() {
    return this.stage === "ready";
  }
  observeInitial(display: number, list: RemoteDisplay[]) {
    if (this.stage !== "ready" || this.target !== null) return false;
    const found = list.find((d) => d.id === display);
    if (!found) return false;
    this.choice = { ...found };
    this.target = found.id;
    return true;
  }
  resolve(list: RemoteDisplay[]): DisplayTicket | null {
    if (this.stage !== "waiting_list" || !list.length) return null;
    if (this.choice) {
      const match = matchRetainedDisplay(this.choice, list);
      if (match.kind !== "matched") {
        this.stage = "unavailable";
        this.issue = match.kind;
        return null;
      }
      return this.choose(match.display);
    }
    return this.choose(list.find((d) => d.id === 1) ?? list[0]);
  }
  choose(display: RemoteDisplay): DisplayTicket | null {
    if (
      this.stage === "stopped" ||
      this.stage === "selecting" ||
      this.stage === "awaiting_frame"
    )
      return null;
    this.epoch++;
    this.choice = { ...display };
    this.target = display.id;
    this.stage = "selecting";
    this.issue = null;
    return { epoch: this.epoch, display: display.id };
  }
  current(ticket: DisplayTicket) {
    return (
      this.stage !== "stopped" &&
      ticket.epoch === this.epoch &&
      ticket.display === this.target
    );
  }
  requested(ticket: DisplayTicket) {
    if (!this.current(ticket) || this.stage !== "selecting") return false;
    this.stage = "awaiting_frame";
    return true;
  }
  fail(ticket: DisplayTicket, issue: "request" | "timeout") {
    if (
      !this.current(ticket) ||
      (this.stage !== "selecting" && this.stage !== "awaiting_frame")
    )
      return false;
    this.stage = "unavailable";
    this.issue = issue;
    return true;
  }
  acceptFrame(display: number) {
    if (this.stage === "awaiting_frame" && display === this.target) {
      this.stage = "ready";
      return true;
    }
    return (
      this.stage === "ready" &&
      (this.target === null || display === this.target)
    );
  }
  stop() {
    this.epoch++;
    this.stage = "stopped";
  }
}

// Both queue entry and rejection delivery belong to the same live input
// epoch. A delayed failure must not close a session already being recovered.
export async function dispatchViewerInput(
  send: () => Promise<unknown>,
  current: () => boolean,
  failed: (error: unknown) => void,
) {
  if (!current()) return;
  try {
    await send();
  } catch (error) {
    if (current()) failed(error);
  }
}

export type RecoveryStatus = {
  cycle: number;
  phase:
    | "idle"
    | "backoff"
    | "requesting"
    | "awaiting_approval"
    | "connecting"
    | "connected"
    | "failed"
    | "cancelled";
  attempt: number;
  maxAttempts: number;
  retryInMs: number | null;
  manualRetryAllowed: boolean;
};
export function recoveryStageText(status: RecoveryStatus | null) {
  const attempt = status?.attempt
    ? `第 ${status.attempt}/${status.maxAttempts} 次`
    : "最多 3 次";
  switch (status?.phase) {
    case "backoff":
      return `正在重连（${attempt}），${Math.ceil((status.retryInMs ?? 0) / 1000)} 秒后尝试`;
    case "requesting":
      return `正在重连（${attempt}），正在请求新的授权`;
    case "awaiting_approval":
      return `正在重连（${attempt}），等待对方批准`;
    case "connecting":
      return `正在重连（${attempt}），正在建立连接`;
    case "cancelled":
      return "重连已取消或授权已结束";
    default:
      return `正在恢复连接（${attempt}）`;
  }
}
