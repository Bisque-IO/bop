// src/components/layout/Topbar.tsx
import React from "react";
import { Menu } from "lucide-react";
import { useSidebar } from "./sidebar-context";

export default function Topbar() {
  const { toggle } = useSidebar();

  return (
    <div className="h-14 border-b border-zinc-800 flex items-center justify-between px-4 bg-zinc-950/60 backdrop-blur">
      <div className="flex items-center gap-2">
        <button
          onClick={toggle}
          className="inline-flex items-center justify-center w-9 h-9 rounded-md hover:bg-zinc-800/60"
          title="Toggle sidebar"
        >
          <Menu className="w-4 h-4 opacity-80" />
        </button>
        <div className="font-semibold ml-1">
          <div className="shrink-0 flex w-full items-center px-1 py-2 relative" style={{left: -15}}>
            <svg xmlns="http://www.w3.org/2000/svg" width="50" height="50" viewBox="0 0 1024 1024" shape-rendering="geometricPrecision">
              <path d="M 212 245 L 213 644 L 321 643 L 322 429 L 417 597 L 471 598 L 563 428 L 564 596 L 669 700 L 732 701 L 680 754 L 711 785 L 825 672 L 721 568 L 690 598 L 756 663 L 704 664 L 672 631 L 671 245 L 540 245 L 443 442 L 343 245 Z" fill="white"/>
            </svg>
        {/* <span className="font-semibold text-lg tracking-tight relative" style={{left: -10}}>OVD</span> */}
          </div>
        </div>
      </div>
      <div className="text-xs text-zinc-400">
        vertx cluster • raft • monitoring
      </div>
    </div>
  );
}
