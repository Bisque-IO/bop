import React from "react";
import { useAuth } from "../../auth/AuthProvider";
import { Avatar, AvatarFallback } from "../ui/avatar";
import { Button } from "../ui/button";
import { LogOut, User as UserIcon } from "lucide-react";

export default function SidebarUser() {
  const { user, logout } = useAuth();
  const name = user?.name ?? user?.email?.split("@")[0] ?? "User";
  const email = user?.email ?? "";

  return (
    <div className="sticky bottom-0 pb-2 pt-1 inset-x-0 border-t border-border bg-zinc-900/70 backdrop-blur supports-[backdrop-filter]:bg-zinc-900/50">
      <div className="px-2 py-2 sm:px-3 flex items-center gap-2">
        <Avatar className="h-8 w-8">
          <AvatarFallback className="text-xs">
            {name.slice(0,1).toUpperCase()}
          </AvatarFallback>
        </Avatar>
        <div className="min-w-0 flex-1 leading-tight">
          <div className="truncate text-sm font-medium text-foreground/90">{name}</div>
          <div className="truncate text-[11px] text-muted-foreground">{email}</div>
        </div>
        <Button
          size="sm"
          variant="ghost"
          className="shrink-0"
          onClick={async () => { await logout(); location.assign("/login"); }}
          aria-label="Log out"
        >
          <LogOut className="mr-2 h-4 w-4" />
          {/* <span>Logout</span> */}
        </Button>
      </div>
    </div>
  );
}
