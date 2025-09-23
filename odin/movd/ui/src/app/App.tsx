// import React, { Suspense, lazy, useEffect } from "react";
// import { HashRouter, Routes, Route, useNavigate } from "react-router-dom";
// import Layout from "@/components/layout/Layout";

// const Overview   = lazy(() => import("@/pages/Overview"));
// const Schemas    = lazy(() => import("@/pages/Schemas"));
// const Monitoring = lazy(() => import("@/pages/Monitoring"));
// const Raft       = lazy(() => import("@/pages/Raft"));
// const Logs       = lazy(() => import("@/pages/Logs/LogsPage"));
// const Code       = lazy(() => import("@/pages/Code/CodePage"));
// const Settings   = lazy(() => import("@/pages/Settings"));

// function NotFound(){
//   const nav = useNavigate();
//   return (
//     <div className="grid place-items-center h-[60vh]">
//       <div className="text-center grid gap-3">
//         <div className="text-4xl font-semibold">404</div>
//         <div className="text-zinc-400">Page not found</div>
//         <button className="px-3 py-2 rounded-xl bg-zinc-800" onClick={()=>nav("/")}>Go home</button>
//       </div>
//     </div>
//   );
// }

// function DevAssertions(){
//   useEffect(() => {
//     if (!(import.meta as any).env?.DEV) return;
//     void import("@/lib/mock").then(({ generateLogs }) => {
//       const testLogs = generateLogs();
//       console.assert(Array.isArray(testLogs) && testLogs.length === 1000, "generateLogs should return 1000 rows");
//       console.assert(["DEBUG","INFO","WARN","ERROR"].includes(testLogs[0].level), "log level valid");
//     }).catch((e)=>console.warn("Dev test (mock) failed:", e));
//     void import("@/lib/utils").then(({ renderDDL }) => {
//       const ddl = renderDDL({ name: "users", columns: [{ name: "id", type: "uuid", nullable: false }] });
//       console.assert(/CREATE TABLE users/.test(ddl), "DDL contains table");
//     }).catch((e)=>console.warn("Dev test (utils) failed:", e));
//   }, []);
//   return null;
// }

// export default function App(){
//   return (
//     <HashRouter>
//       <Layout>
//         <DevAssertions />
//         <Suspense fallback={<div className="p-4 text-sm text-zinc-400">Loading…</div>}>
//           <Routes>
//             <Route path="/" element={<Overview/>} />
//             <Route path="/schemas" element={<Schemas/>} />
//             <Route path="/monitoring" element={<Monitoring/>} />
//             <Route path="/raft" element={<Raft/>} />
//             <Route path="/logs" element={<Logs/>} />
//             <Route path="/code" element={<Code/>} />
//             <Route path="/settings" element={<Settings/>} />
//             <Route path="*" element={<NotFound/>} />
//           </Routes>
//         </Suspense>
//       </Layout>
//     </HashRouter>
//   );
// }

import React from "react";
import { Routes, Route, Navigate } from "react-router-dom";
import ProtectedRoute from "../auth/ProtectedRoute";
import Login from "../pages/Login";

// Existing pages
import Overview from "../pages/Overview";
import LogsPage from "../pages/Logs/LogsPage";
import Monitoring from "../pages/Monitoring";
import Schemas from "../pages/Schemas";
import Settings from "../pages/Settings";
import Raft from "../pages/Raft";

import Layout from "@/components/layout/Layout";
import CodePage from "@/pages/Code/CodePage";

export default function App() {
  return (
    <Routes>
      <Route path="/login" element={<Login />} />

      {/* Protected area */}
      <Route element={<ProtectedRoute />}>
        <Route element={<Layout />}> {/* keeps sidebar/topbar */}
          <Route index element={<Overview />} />
          <Route path="/logs" element={<LogsPage />} />
          <Route path="/monitoring" element={<Monitoring />} />
          <Route path="/schemas" element={<Schemas />} />
          <Route path="/raft" element={<Raft />} />
          <Route path="/code" element={<CodePage />} />
          <Route path="/settings" element={<Settings />} />
        </Route>
      </Route>

      {/* Fallbacks */}
      <Route path="/" element={<Navigate to="/" replace />} />
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  );
}