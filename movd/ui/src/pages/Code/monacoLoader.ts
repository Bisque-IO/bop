import { loader } from "@monaco-editor/react";
export function configureMonacoCDN() {
  try {
    loader.config({ paths: { vs: "https://cdn.jsdelivr.net/npm/monaco-editor@0.47.0/min/vs" } });
  } catch {}
}
export { loader };
export function registerAssemblyScriptLib(monaco: any) {
  const asLib = `
  /// <reference no-default-lib="true"/>
  declare type i8 = number; declare type i16 = number; declare type i32 = number; declare type i64 = number;
  declare type u8 = number; declare type u16 = number; declare type u32 = number; declare type u64 = number;
  declare type isize = number; declare type usize = number; declare type f32 = number; declare type f64 = number; declare type bool = boolean;
  declare function abort(message?: string | null, fileName?: string | null, lineNumber?: i32, columnNumber?: i32): void;
  declare namespace console { export function log(...args: any[]): void }
  declare const memory: WebAssembly.Memory;
  `;
  monaco.languages.typescript.typescriptDefaults.addExtraLib(asLib, "ts:assemblyscript.d.ts");
}
