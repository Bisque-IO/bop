import React from "react";
export function Card({ className = "", children }: React.PropsWithChildren<{ className?: string }>) {
  return <div className={"rounded-2xl border " + className}>{children}</div>;
}
export function CardHeader({ className = "", children }: React.PropsWithChildren<{ className?: string }>) {
  return <div className={"px-4 py-3 border-b border-zinc-800 " + className}>{children}</div>;
}
export function CardTitle({ children, className="" }: React.PropsWithChildren<{ className?: string }>) {
  return <h3 className={"text-lg font-semibold " + className}>{children}</h3>;
}
export function CardContent({ className = "", children }: React.PropsWithChildren<{ className?: string }>) {
  return <div className={"px-4 py-3 " + className}>{children}</div>;
}
