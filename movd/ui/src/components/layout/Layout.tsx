// src/components/layout/Layout.tsx
import React, { useEffect, useRef, useState } from "react";
import Topbar from "./Topbar";
import Sidebar from "./Sidebar";
import {
  PanelGroup,
  Panel,
  PanelResizeHandle,
  type ImperativePanelHandle,
} from "react-resizable-panels";
import { SidebarCtx } from "./sidebar-context";
import { Outlet } from "react-router-dom";

export default function Layout({ children }: { children: React.ReactNode }) {
  const sidebarRef = useRef<ImperativePanelHandle>(null);
  const [collapsed, setCollapsed] = useState(false);

  // Keep React state in sync with the real panel state on mount
  useEffect(() => {
    const api = sidebarRef.current;
    if (api && typeof api.isCollapsed === "function") {
      try {
        setCollapsed(!!api.isCollapsed());
      } catch {}
    }
  }, []);

  const controller = {
    collapsed,
    toggle: () => {
      const api = sidebarRef.current;
      if (!api) return;
      // Query the *actual* state from the panel, not our React state.
      const isNowCollapsed =
        typeof api.isCollapsed === "function" ? !!api.isCollapsed() : collapsed;
      if (isNowCollapsed) api.expand?.();
      else api.collapse?.();
    },
    collapse: () => sidebarRef.current?.collapse?.(),
    expand: () => sidebarRef.current?.expand?.(),
  };

  return (
    <SidebarCtx.Provider value={controller}>
      <div className="min-h-0 h-full bg-zinc-950 text-zinc-200">
        <PanelGroup
          direction="horizontal"
          className="h-screen"
          autoSaveId="main-layout"
        >
          <Panel
            ref={sidebarRef}
            order={1}
            defaultSize={20}
            minSize={12}
            maxSize={35}
            collapsible
            collapsedSize={0}
            onCollapse={() => setCollapsed(true)}
            onExpand={() => setCollapsed(false)}
          >
            <Sidebar />
          </Panel>

          {/* Keep the handle mounted; optionally make it draggable when collapsed by using w-1 instead of w-0 */}
          <PanelResizeHandle
            className={`transition-colors cursor-col-resize ${
              collapsed
                ? "w-1"
                : "w-1 bg-zinc-900 hover:bg-zinc-800 active:bg-zinc-700"
            }`}
          />

          <Panel order={2}>
            <div className="flex flex-col h-screen min-h-0">
              <Topbar />
              <main className="h-full p-4 gap-4 flex-1 overflow-hidden min-h-0">
              {/* <div className="h-full min-h-0 overflow-hidden"> */}
                <Outlet />
              </main>
            </div>
          </Panel>
        </PanelGroup>
      </div>
    </SidebarCtx.Provider>
  );
}
