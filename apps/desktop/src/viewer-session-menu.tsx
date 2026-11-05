import {
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import {
  ChevronDown16Regular,
  Desktop20Regular,
  FullScreenMinimize20Regular,
} from "@fluentui/react-icons";

export type ViewerTab = {
  instance: string;
  id: string;
  name: string;
  selected: boolean;
};

export function ViewerSessionMenu({
  tabs,
  onSelect,
  onDetach,
  fullscreen = false,
  onExitFullscreen,
}: {
  tabs: ViewerTab[];
  onSelect: (tab: ViewerTab) => void;
  onDetach?: (tab: ViewerTab) => void;
  fullscreen?: boolean;
  onExitFullscreen?: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState({
    left: 12,
    top: 48,
    width: 280,
    maxHeight: 300,
  });
  const trigger = useRef<HTMLButtonElement>(null);
  const items = useRef<Array<HTMLButtonElement | null>>([]);
  const openingIndex = useRef<number | null>(null);
  const listId = useId();
  const labelId = useId();
  const descriptionId = useId();
  const selected = tabs.find((tab) => tab.selected);
  const selectedInstance = selected?.instance;
  const selectedIndex = Math.max(
    0,
    tabs.findIndex((tab) => tab.selected),
  );

  const close = useCallback((restoreFocus = true) => {
    setOpen(false);
    if (restoreFocus) trigger.current?.focus({ preventScroll: true });
  }, []);

  // Selection can also change through the workspace shortcuts or native state.
  // Closing here never moves focus away from the newly selected remote screen.
  useEffect(() => setOpen(false), [selectedInstance, selected?.id]);

  useLayoutEffect(() => {
    if (!open) return;
    const place = () => {
      const rect = trigger.current?.getBoundingClientRect();
      if (!rect) return;
      const width = Math.min(280, Math.max(0, window.innerWidth - 24));
      const top = rect.bottom + 5;
      setPosition({
        left: Math.max(12, Math.min(rect.left, window.innerWidth - width - 12)),
        top,
        width,
        maxHeight: Math.max(0, window.innerHeight - top - 12),
      });
    };
    place();
    window.addEventListener("resize", place);
    return () => window.removeEventListener("resize", place);
  }, [open, fullscreen, tabs.length, selected?.name]);

  useLayoutEffect(() => {
    if (!open) return;
    const index = openingIndex.current ?? selectedIndex;
    openingIndex.current = null;
    items.current[index]?.focus({ preventScroll: true });
    items.current[index]?.scrollIntoView({ block: "nearest" });
  }, [open]);

  const navigate = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape" && open) {
      event.preventDefault();
      event.stopPropagation();
      close();
      return;
    }
    if (!open || !tabs.length) return;
    const current = items.current.findIndex((item) => item === event.target);
    if (current < 0) return;
    let next: number;
    if (event.key === "ArrowDown") next = (current + 1) % tabs.length;
    else if (event.key === "ArrowUp")
      next = (current + tabs.length - 1) % tabs.length;
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = tabs.length - 1;
    else return;
    event.preventDefault();
    event.stopPropagation();
    items.current[next]?.focus({ preventScroll: true });
    items.current[next]?.scrollIntoView({ block: "nearest" });
  };

  return (
    <div
      className={`viewer-tab-menu${fullscreen ? " viewer-fullscreen-switcher" : ""}`}
      onKeyDown={navigate}
      onBlurCapture={(event) => {
        if (
          event.relatedTarget instanceof Node &&
          event.currentTarget.contains(event.relatedTarget)
        )
          return;
        close(false);
      }}
    >
      <button
        ref={trigger}
        type="button"
        className="viewer-tab-menu-trigger"
        aria-label="切换远程设备"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-describedby={fullscreen ? descriptionId : undefined}
        title="切换远程设备"
        disabled={!tabs.length}
        onClick={() => {
          if (open) close();
          else setOpen(true);
        }}
        onKeyDown={(event) => {
          if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key))
            return;
          event.preventDefault();
          event.stopPropagation();
          const index =
            event.key === "Home"
              ? 0
              : event.key === "End"
                ? tabs.length - 1
                : selectedIndex;
          if (open) {
            items.current[index]?.focus({ preventScroll: true });
            items.current[index]?.scrollIntoView({ block: "nearest" });
          } else {
            openingIndex.current = index;
            setOpen(true);
          }
        }}
      >
        {fullscreen && (
          <span id={descriptionId} className="viewer-fullscreen-device-name">
            <span>{selected?.name ?? "远程设备"}</span>
            <span className="viewer-session-count">{tabs.length}</span>
          </span>
        )}
        <ChevronDown16Regular aria-hidden="true" />
      </button>
      {fullscreen && onExitFullscreen && (
        <button
          type="button"
          className="viewer-exit-fullscreen"
          aria-label="退出全屏"
          title="退出全屏"
          onClick={() => {
            close(false);
            onExitFullscreen();
          }}
        >
          <FullScreenMinimize20Regular aria-hidden="true" />
        </button>
      )}
      {open && (
        <>
          <div
            className="viewer-tab-menu-backdrop"
            aria-hidden="true"
            onPointerDown={(event) => {
              // Keep focus in the popup and consume the complete pointer gesture.
              event.preventDefault();
              event.stopPropagation();
            }}
            onPointerUp={(event) => event.stopPropagation()}
            onClick={(event) => {
              event.preventDefault();
              event.stopPropagation();
              close();
            }}
          />
          <div
            id={listId}
            className="viewer-tab-menu-list"
            role="group"
            aria-labelledby={labelId}
            style={{ position: "fixed", ...position }}
          >
            <strong id={labelId}>已连接设备 · {tabs.length}</strong>
            {tabs.map((tab, index) => (
              <button
                key={tab.instance}
                ref={(node) => {
                  items.current[index] = node;
                }}
                type="button"
                aria-label={tab.name}
                aria-current={tab.selected ? "true" : undefined}
                onClick={(event) => {
                  event.stopPropagation();
                  close();
                  onSelect(tab);
                }}
              >
                <Desktop20Regular aria-hidden="true" />
                <span>{tab.name}</span>
              </button>
            ))}
            {onDetach && tabs.length > 1 && selected && (
              <button type="button" onClick={() => { close(); onDetach(selected); }}>
                当前设备在独立窗口打开
              </button>
            )}
          </div>
        </>
      )}
    </div>
  );
}
