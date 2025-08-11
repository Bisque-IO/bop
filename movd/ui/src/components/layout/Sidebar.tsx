import React from "react";
import { NavLink } from "react-router-dom";
import { LayoutDashboard, Database, Activity, Network, Logs, Code2, Settings } from "lucide-react";
import SidebarUser from "./SidebarUser";

const link = "flex items-center gap-2 px-3 py-2 rounded-xl hover:bg-zinc-800/60";
const active = "bg-zinc-800/60 text-zinc-50";

export default function Sidebar(){
  return (
    <aside className="hidden lg:flex lg:flex-col w-full shrink-0 h-screen border-zinc-800 bg-zinc-950/60">
      <div className="shrink-0 flex w-full items-center px-1 py-1">
        <svg xmlns="http://www.w3.org/2000/svg" width="50" height="50" viewBox="0 0 1024 1024" shape-rendering="geometricPrecision">
          <path d="M 212 245 L 213 644 L 321 643 L 322 429 L 417 597 L 471 598 L 563 428 L 564 596 L 669 700 L 732 701 L 680 754 L 711 785 L 825 672 L 721 568 L 690 598 L 756 663 L 704 664 L 672 631 L 671 245 L 540 245 L 443 442 L 343 245 Z" fill="white"/>
        </svg>
        <span className="font-semibold text-lg tracking-tight relative" style={{left: -10}}>OVD</span>
      </div>

      <div className="flex-1 min-h-0 overflow-y-auto no-scrollbar p-3 gap-2 relative" style={{top: -15}}>
        <NavLink to="/" end className={({isActive})=>link + " " + (isActive?active:"")}><LayoutDashboard className="w-4 h-4"/>Overview</NavLink>
        <NavLink to="/schemas" className={({isActive})=>link + " " + (isActive?active:"")}><Database className="w-4 h-4"/>Schemas</NavLink>
        <NavLink to="/monitoring" className={({isActive})=>link + " " + (isActive?active:"")}><Activity className="w-4 h-4"/>Monitoring</NavLink>
        <NavLink to="/raft" className={({isActive})=>link + " " + (isActive?active:"")}><Network className="w-4 h-4"/>Raft</NavLink>
        <NavLink to="/logs" className={({isActive})=>link + " " + (isActive?active:"")}><Logs className="w-4 h-4"/>Logs</NavLink>
        <NavLink to="/code" className={({isActive})=>link + " " + (isActive?active:"")}><Code2 className="w-4 h-4"/>Code</NavLink>
        <NavLink to="/settings" className={({isActive})=>link + " " + (isActive?active:"")}><Settings className="w-4 h-4"/>Settings</NavLink>
      </div>
      
      <div className="sticky bottom-0">
        <SidebarUser /> {/* make SidebarUser a plain block; no sticky inside */}
      </div>
    </aside>
  );
}
