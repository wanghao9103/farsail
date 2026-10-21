import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Device, Remote } from "@farsail/ui";
import { ArrowUpload20Regular, Folder20Regular } from "@fluentui/react-icons";
import { PageActions } from "./page-actions";

type FileState = { deviceId: string | null; filesEnabled?: boolean };
type FileAction = (
  work: () => Promise<unknown>,
  message: string,
  reload?: boolean,
) => Promise<void>;
type Transfer = {
  id: string;
  name: string;
  size: number;
  transferred: number;
  direction: "send" | "receive";
  state:
    | "offered"
    | "transferring"
    | "completed"
    | "rejected"
    | "cancelled"
    | "failed";
  error?: string | null;
};
type FileSession = {
  permission?: "files" | "control";
  id: string;
  host: boolean;
  state: "connected" | "closed";
  path: "direct" | "relay" | "none";
  error?: string | null;
  transfers: Transfer[];
};
type FileSnapshot = { enabled: boolean; sessions: FileSession[] };
const sizeText = (bytes: number) => {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
};
const transferText = (transfer: Transfer) =>
  ({
    offered:
      transfer.direction === "receive" ? "等待你选择保存位置" : "等待对方接收",
    transferring: transfer.direction === "receive" ? "正在接收" : "正在发送",
    completed: "传输完成",
    rejected: "已拒绝",
    cancelled: "已取消",
    failed: "传输失败",
  })[transfer.state];

export function FilesPanel({
  requests,
  devices,
  state,
  selectedId,
  onSelect,
  busy,
  act,
  prepareTransport,
  onRefresh,
}: {
  requests: Remote[];
  devices: Device[];
  state: FileState;
  selectedId: string | null;
  onSelect: (id: string) => void;
  busy: boolean;
  act: FileAction;
  prepareTransport: () => Promise<void>;
  onRefresh: () => Promise<void>;
}) {
  const [snapshot, setSnapshot] = useState<FileSnapshot>({
    enabled: false,
    sessions: [],
  });
  const [readError, setReadError] = useState("");
  const readVersion = useRef(0);
  const reading = useRef(false);
  const refresh = useCallback(async () => {
    if (reading.current) return;
    reading.current = true;
    const version = readVersion.current;
    try {
      const next = await invoke<FileSnapshot>("files_status");
      if (version !== readVersion.current) return;
      if (!next || !Array.isArray(next.sessions))
        throw new Error("无法读取文件传输状态，请确认客户端版本。");
      setSnapshot({
        ...next,
        sessions: next.sessions.filter(
          (session) => session.permission !== "control",
        ),
      });
      setReadError("");
    } catch (failure) {
      if (version === readVersion.current) setReadError(String(failure));
    } finally {
      reading.current = false;
    }
  }, []);
  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 750);
    return () => {
      readVersion.current++;
      window.clearInterval(timer);
    };
  }, [refresh]);
  const fileRequests = requests.filter(
    (request) =>
      request.permission === "files" &&
      (request.source_device_id === state.deviceId ||
        request.target_device_id === state.deviceId) &&
      ["pending", "approved"].includes(request.state),
  );
  const ids = Array.from(
    new Set([
      ...fileRequests.map((request) => request.id),
      ...snapshot.sessions.map((session) => session.id),
    ]),
  );
  const currentId =
    selectedId && ids.includes(selectedId) ? selectedId : ids[0];
  const request = fileRequests.find((item) => item.id === currentId);
  const session = snapshot.sessions.find((item) => item.id === currentId);
  const peerName = (id: string) => {
    const request = requests.find((item) => item.id === id);
    const peer =
      request &&
      (request.source_device_id === state.deviceId
        ? request.target_device_id
        : request.source_device_id);
    return devices.find((item) => item.id === peer)?.name ?? "文件连接";
  };
  const connected = session?.state === "connected";
  const command = async (name: string, transferId?: string) => {
    await invoke(name, {
      id: currentId,
      ...(transferId ? { transferId } : {}),
    });
    await refresh();
  };
  return (
    <section className="files-workspace">
      <PageActions>
        <button
          className="text-button"
          disabled={busy}
          onClick={() =>
            void act(
              async () => {
                await onRefresh();
                await refresh();
              },
              "文件状态已刷新",
              false,
            )
          }
        >
          刷新文件状态
        </button>
      </PageActions>
      {readError && (
        <div className="alert" role="alert">
          无法读取文件状态：{readError}
        </div>
      )}
      <div className="files-layout">
        <div
          className="card file-session-list"
          role="region"
          tabIndex={0}
          aria-label="文件连接列表"
        >
          <h2>文件连接</h2>
          {ids.map((id) => (
            <button
              key={id}
              className={
                currentId === id ? "file-session selected" : "file-session"
              }
              aria-pressed={currentId === id}
              onClick={() => onSelect(id)}
            >
              <Folder20Regular aria-hidden="true" />
              <span>
                {peerName(id)}
                <small>
                  {snapshot.sessions.find((item) => item.id === id)?.state ===
                  "connected"
                    ? "已连接"
                    : snapshot.sessions.find((item) => item.id === id)
                          ?.state === "closed"
                      ? "已结束"
                      : fileRequests.find((item) => item.id === id)?.state ===
                          "pending"
                        ? "等待批准"
                        : "尚未连接"}
                </small>
              </span>
            </button>
          ))}
          {!ids.length && <p className="muted">在“我的设备”中发起文件连接。</p>}
        </div>
        <div className="card file-transfer-pane">
          <div className="section-heading">
            <div>
              <h2>{currentId ? peerName(currentId) : "传输记录"}</h2>
              {currentId && (
                <p className="muted" role="status">
                  {connected
                    ? session.path === "direct"
                      ? "已直连"
                      : session.path === "relay"
                        ? "已通过中继连接"
                        : "已连接 · 正在确认传输路径"
                    : session?.state === "closed"
                      ? "文件连接已结束"
                      : request?.state === "pending"
                        ? "等待对方批准文件连接"
                        : request?.state === "approved"
                          ? "已批准，等待建立文件连接"
                          : "尚未连接"}
                </p>
              )}
            </div>
            <div className="row-actions">
              {request?.state === "approved" &&
                request.source_device_id === state.deviceId &&
                !session && (
                  <button
                    className="secondary"
                    disabled={busy}
                    onClick={() =>
                      void act(
                        async () => {
                          await prepareTransport();
                          await invoke("transport_connect", {
                            id: currentId,
                            permission: "files",
                          });
                          await refresh();
                        },
                        "文件连接已建立",
                        false,
                      )
                    }
                  >
                    连接文件传输
                  </button>
                )}
              <button
                className="primary"
                disabled={busy || !connected}
                onClick={() => void act(() => command("files_send"), "", false)}
              >
                <ArrowUpload20Regular aria-hidden="true" />
                选择文件发送
              </button>
              {request && ["pending", "approved"].includes(request.state) && (
                <button
                  className="danger-text"
                  disabled={busy}
                  onClick={() =>
                    void act(
                      async () => {
                        await invoke("call", {
                          op: "revoke_remote",
                          args: { id: currentId },
                        });
                        await onRefresh();
                        await refresh();
                      },
                      request.state === "pending"
                        ? "文件连接请求已取消"
                        : "文件连接已结束",
                      false,
                    )
                  }
                >
                  {request.state === "pending" ? "取消请求" : "结束文件连接"}
                </button>
              )}
            </div>
          </div>
          {session?.error && (
            <p className="hint" role="alert">
              {session.error}
            </p>
          )}
          <div
            className="file-transfer-list"
            role="region"
            tabIndex={0}
            aria-label="文件传输记录"
          >
            {session?.transfers.map((transfer) => (
              <article
                className="file-transfer"
                key={transfer.id}
                aria-label={`${transfer.direction === "receive" ? "接收" : "发送"} ${transfer.name}`}
              >
                <div className="file-transfer-description">
                  <strong>{transfer.name}</strong>
                  <small>
                    {transfer.direction === "receive" ? "接收" : "发送"} ·{" "}
                    {sizeText(transfer.size)} · {transferText(transfer)}
                  </small>
                  {transfer.state === "transferring" && (
                    <>
                      <progress
                        aria-label={`${transfer.name} 传输进度`}
                        max={Math.max(1, transfer.size)}
                        value={transfer.transferred}
                      />
                      <small>
                        {sizeText(transfer.transferred)} /{" "}
                        {sizeText(transfer.size)}
                      </small>
                    </>
                  )}
                  {transfer.error && (
                    <p className="hint" role="alert">
                      {transfer.error}
                    </p>
                  )}
                </div>
                <div className="row-actions">
                  {transfer.direction === "receive" &&
                    transfer.state === "offered" && (
                      <>
                        <button
                          className="secondary"
                          disabled={busy || !connected}
                          onClick={() =>
                            void act(
                              () => command("files_reject", transfer.id),
                              "已拒绝接收文件",
                              false,
                            )
                          }
                        >
                          拒绝
                        </button>
                        <button
                          className="primary"
                          disabled={busy || !connected}
                          onClick={() =>
                            void act(
                              () => command("files_accept", transfer.id),
                              "",
                              false,
                            )
                          }
                        >
                          保存到…
                        </button>
                      </>
                    )}
                  {((transfer.state === "offered" &&
                    transfer.direction === "send") ||
                    transfer.state === "transferring") && (
                    <button
                      className="danger-text"
                      disabled={busy}
                      onClick={() =>
                        void act(
                          () => command("files_cancel", transfer.id),
                          "传输已取消",
                          false,
                        )
                      }
                    >
                      取消传输
                    </button>
                  )}
                </div>
              </article>
            ))}
            {!session?.transfers.length && (
              <p className="muted">暂无传输记录。</p>
            )}
          </div>
        </div>
      </div>
    </section>
  );
}
