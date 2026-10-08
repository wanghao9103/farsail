import { useEffect, useRef, useId } from "react";
import { DialogKeyboardGate } from "./feedback-policy";
import {
  ErrorCircle24Filled,
  Dismiss16Regular,
  Warning24Regular,
} from "@fluentui/react-icons";

export type FailureAction = {
  title?: string;
  retry?: () => void;
  retryLabel?: string;
};

const dialogKeys = new DialogKeyboardGate();
let keyboardBoundaryInstalled = false;
function installKeyboardBoundary() {
  if (keyboardBoundaryInstalled) return;
  keyboardBoundaryInstalled = true;
  const filter = (event: KeyboardEvent) => {
    const insideDialog =
      event.target instanceof Element && !!event.target.closest("dialog[open]");
    if (insideDialog && event.type === "keydown")
      dialogKeys.press(event.code || event.key, event.repeat);
    if (
      dialogKeys.suppress(
        event.code || event.key,
        event.type === "keyup" ? "up" : "down",
        insideDialog,
      )
    ) {
      event.preventDefault();
      event.stopImmediatePropagation();
    }
  };
  window.addEventListener("keydown", filter, true);
  window.addEventListener("keyup", filter, true);
  window.addEventListener("blur", () => dialogKeys.reset());
}

export function failureTitle(raw: string) {
  if (/transport timeout/i.test(raw)) return "连接远程电脑超时";
  if (/relay.*reachable|relay URL/i.test(raw)) return "无法连接中继服务器";
  if (/HTTP 401|signed out|sign in first/i.test(raw)) return "登录已失效";
  if (/HTTP 403|permission rejected/i.test(raw)) return "无法执行此操作";
  if (/HTTP 404/i.test(raw)) return "未找到设备或记录";
  if (/自动重连未成功/.test(raw)) return "暂时无法恢复连接";
  if (/session closed|会话已结束|已结束连接/.test(raw)) return "远程连接已结束";
  return "操作未完成";
}

export function failureMessage(raw: string) {
  if (/transport timeout/i.test(raw))
    return "暂时未能与远程电脑建立连接。你可以重新连接，或稍后再试。";
  if (/relay.*reachable|relay URL/i.test(raw))
    return "暂时无法使用中继连接。请稍后重试，或检查连接设置中的中继地址。";
  if (/HTTP 401|signed out|sign in first/i.test(raw))
    return "请重新登录后再继续。";
  if (/HTTP 403|permission rejected/i.test(raw))
    return "当前授权不允许执行此操作，请重新申请连接或联系设备所有者。";
  if (/HTTP 404/i.test(raw)) return "该设备或记录可能已被移除，请刷新后再试。";
  if (/HTTP 409/i.test(raw)) return "状态已发生变化，请刷新后再试。";
  if (/HTTP 429/i.test(raw)) return "请求较多，请稍后再试。";
  if (/[\u3400-\u9fff]/.test(raw)) return raw;
  return "未能完成本次操作。请稍后重试，技术详情可帮助定位问题。";
}

export function FailureDialog({
  error,
  onDismiss,
  action,
  message,
  confirmation = false,
}: {
  error: string;
  onDismiss: () => void;
  action?: FailureAction;
  message?: string;
  confirmation?: boolean;
}) {
  const dialog = useRef<HTMLDialogElement>(null);
  const description = useId();
  const title = action?.title ?? failureTitle(error);
  useEffect(() => {
    installKeyboardBoundary();
    const node = dialog.current;
    if (!error || !node) return;
    const previous = document.activeElement;
    if (!node.open) node.showModal();
    return () => {
      if (node.open) node.close();
      if (previous instanceof HTMLElement && previous.isConnected)
        previous.focus({ preventScroll: true });
    };
  }, [error]);
  if (!error) return null;
  return (
    <dialog
      ref={dialog}
      className={`failure-dialog${confirmation ? " confirmation-dialog" : ""}`}
      role="alertdialog"
      aria-label={title}
      aria-describedby={description}
      onCancel={(e) => {
        e.preventDefault();
        onDismiss();
      }}
      onKeyDown={(e) => {
        e.stopPropagation();
      }}
      onKeyUp={(e) => e.stopPropagation()}
    >
      <button
        className="dialog-close"
        aria-label={confirmation ? "关闭确认" : "关闭错误"}
        onClick={onDismiss}
      >
        <Dismiss16Regular />
      </button>
      <div className="failure-dialog-content">
        {confirmation ? (
          <Warning24Regular className="failure-symbol" aria-hidden="true" />
        ) : (
          <ErrorCircle24Filled className="failure-symbol" aria-hidden="true" />
        )}
        <div>
          <h2>{title}</h2>
          <p id={description}>{message ?? failureMessage(error)}</p>
        </div>
      </div>
      <div className="failure-dialog-actions">
        {action?.retry ? (
          <>
            <button
              className="primary"
              autoFocus={!confirmation}
              onClick={() => {
                onDismiss();
                action.retry?.();
              }}
            >
              {action.retryLabel ?? "重试"}
            </button>
            <button
              className="secondary"
              autoFocus={confirmation}
              onClick={onDismiss}
            >
              取消
            </button>
          </>
        ) : (
          <button className="primary" autoFocus onClick={onDismiss}>
            知道了
          </button>
        )}
      </div>
      {!confirmation && (
        <details className="failure-details">
          <summary>技术详情</summary>
          <code>{error}</code>
        </details>
      )}
    </dialog>
  );
}
