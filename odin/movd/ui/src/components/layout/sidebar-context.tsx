import React, { createContext, useContext } from "react";

type SidebarController = {
  collapsed: boolean;
  toggle: () => void;
  collapse: () => void;
  expand: () => void;
};
export const SidebarCtx = createContext<SidebarController | null>(null);

export function useSidebar() {
  const ctx = useContext(SidebarCtx);
  if (!ctx) throw new Error("useSidebar must be used within SidebarCtx provider");
  return ctx;
}
