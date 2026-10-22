import { useLayoutEffect, useState, type ReactNode } from "react";
import { createPortal } from "react-dom";

export function PageActions({ children }: { children: ReactNode }) {
  const [host, setHost] = useState<HTMLElement | null>(null);
  useLayoutEffect(() => {
    setHost(document.getElementById("page-actions"));
  }, []);
  return host ? createPortal(children, host) : null;
}
