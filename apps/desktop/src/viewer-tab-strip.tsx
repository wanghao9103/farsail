import { useId, useLayoutEffect, useRef, useState } from "react";
import {
  ChevronLeft16Regular,
  ChevronRight16Regular,
  Desktop20Regular,
  Dismiss16Regular,
} from "@fluentui/react-icons";
import type { ViewerTab } from "./viewer-session-menu";

export function ViewerTabStrip({
  tabs,
  onSelect,
  onClose,
}: {
  tabs: ViewerTab[];
  onSelect: (tab: ViewerTab) => void;
  onClose: (tab: ViewerTab) => void;
}) {
  const viewport = useRef<HTMLDivElement>(null);
  const listId = useId();
  const [scroll, setScroll] = useState({
    overflow: false,
    left: false,
    right: false,
  });
  const selected = tabs.find((tab) => tab.selected)?.instance;
  const selectedRef = useRef(selected);
  selectedRef.current = selected;

  useLayoutEffect(() => {
    const node = viewport.current;
    if (!node) return;
    const measure = () => {
      const remaining = node.scrollWidth - node.clientWidth;
      const next = {
        overflow: remaining > 1,
        left: node.scrollLeft > 1,
        right: node.scrollLeft < remaining - 1,
      };
      setScroll((old) =>
        old.overflow === next.overflow &&
        old.left === next.left &&
        old.right === next.right
          ? old
          : next,
      );
    };
    const revealSelected = () => {
      const active = document.getElementById(
        `tab-${selectedRef.current}`,
      )?.parentElement;
      if (!active || !node.contains(active)) return;
      const bounds = node.getBoundingClientRect();
      const tab = active.getBoundingClientRect();
      if (tab.left < bounds.left || tab.width > bounds.width)
        node.scrollLeft += tab.left - bounds.left;
      else if (tab.right > bounds.right)
        node.scrollLeft += tab.right - bounds.right;
    };
    const resize = new ResizeObserver(() => {
      revealSelected();
      measure();
    });
    resize.observe(node);
    const wheel = (event: WheelEvent) => {
      if (event.ctrlKey || node.scrollWidth <= node.clientWidth + 1) return;
      const delta =
        Math.abs(event.deltaX) > Math.abs(event.deltaY)
          ? event.deltaX
          : event.deltaY;
      if (!delta) return;
      event.preventDefault();
      event.stopPropagation();
      const unit =
        event.deltaMode === 1
          ? 16
          : event.deltaMode === 2
            ? node.clientWidth
            : 1;
      node.scrollLeft += delta * unit;
      measure();
    };
    node.addEventListener("scroll", measure);
    node.addEventListener("wheel", wheel, { passive: false });
    revealSelected();
    measure();
    return () => {
      resize.disconnect();
      node.removeEventListener("scroll", measure);
      node.removeEventListener("wheel", wheel);
    };
  }, [selected, tabs.length]);

  const scrollByPage = (direction: number) => {
    const node = viewport.current;
    if (!node) return;
    node.scrollBy({
      left: direction * Math.max(84, Math.round(node.clientWidth * 0.8)),
      behavior: "smooth",
    });
  };

  return (
    <div className="viewer-tab-navigation" style={{ width: tabs.length * 218 }}>
      {scroll.overflow && (
        <button
          type="button"
          className="viewer-tab-scroll"
          aria-label="向左滚动设备标签"
          aria-controls={listId}
          title="向左滚动设备标签"
          disabled={!scroll.left}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => scrollByPage(-1)}
        >
          <ChevronLeft16Regular aria-hidden="true" />
        </button>
      )}
      <div
        ref={viewport}
        id={listId}
        className="viewer-tabs"
        role="tablist"
        aria-label="远程设备标签"
      >
        {tabs.map((tab, index) => (
          <div
            key={tab.instance}
            className={`viewer-tab ${tab.selected ? "selected" : ""}`}
          >
            <button
              role="tab"
              id={`tab-${tab.instance}`}
              aria-selected={tab.selected}
              aria-controls={`panel-${tab.instance}`}
              tabIndex={tab.selected ? 0 : -1}
              title={tab.name}
              onClick={() => onSelect(tab)}
              onKeyDown={(event) => {
                if (
                  !["ArrowLeft", "ArrowRight", "Home", "End"].includes(
                    event.key,
                  )
                )
                  return;
                event.preventDefault();
                const next =
                  event.key === "Home"
                    ? tabs[0]
                    : event.key === "End"
                      ? tabs[tabs.length - 1]
                      : tabs[
                          (index +
                            (event.key === "ArrowLeft" ? tabs.length - 1 : 1)) %
                            tabs.length
                        ];
                onSelect(next);
              }}
            >
              <Desktop20Regular
                className="viewer-tab-icon"
                aria-hidden="true"
              />
              <span className="viewer-tab-name">{tab.name}</span>
            </button>
            <button
              type="button"
              className="viewer-tab-close"
              aria-label={`关闭标签 ${tab.name}`}
              title="关闭此连接 (Ctrl+W)"
              onClick={() => onClose(tab)}
            >
              <Dismiss16Regular aria-hidden="true" />
            </button>
          </div>
        ))}
      </div>
      {scroll.overflow && (
        <button
          type="button"
          className="viewer-tab-scroll"
          aria-label="向右滚动设备标签"
          aria-controls={listId}
          title="向右滚动设备标签"
          disabled={!scroll.right}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => scrollByPage(1)}
        >
          <ChevronRight16Regular aria-hidden="true" />
        </button>
      )}
    </div>
  );
}
