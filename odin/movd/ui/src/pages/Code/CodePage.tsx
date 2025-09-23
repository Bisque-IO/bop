import React, { Suspense, useEffect, useRef, useState } from "react";
import type { OnMount } from "@monaco-editor/react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";
import { Save } from "lucide-react";
import { configureMonacoCDN, registerAssemblyScriptLib } from "./monacoLoader";

const MonacoEditor = React.lazy(() => import("@monaco-editor/react"));

class ErrorBoundary extends React.Component<{ fallback: React.ReactNode }, { hasError: boolean }> {
  constructor(props:any){ super(props); this.state = { hasError:false }; }
  static getDerivedStateFromError(){ return { hasError:true }; }
  render(){ return this.state.hasError ? this.props.fallback as any : this.props.children as any; }
}

export default function CodePage(){
  const [value, setValue] = useState<string>(`// AssemblyScript demo\nexport function add(a: i32, b: i32): i32 { return a + b }`);
  const editorRef = useRef<any>(null);
  useEffect(() => { configureMonacoCDN(); }, []);

  const onMount: OnMount = (editor, monaco) => {
    editorRef.current = editor;
    monaco.languages.typescript.typescriptDefaults.setCompilerOptions({
      target: monaco.languages.typescript.ScriptTarget.ES2020,
      module: monaco.languages.typescript.ModuleKind.ESNext,
      noEmit: true,
      allowNonTsExtensions: true,
      allowJs: false, strict: false, baseUrl: "/", jsx: monaco.languages.typescript.JsxEmit.None
    });
    registerAssemblyScriptLib(monaco);
  };
  async function format(){ try { await editorRef.current?.getAction("editor.action.formatDocument")?.run(); } catch {} }
  function save(){ console.log("[CodePage] save", value); }
  useEffect(()=>{ const onKey=(e:KeyboardEvent)=>{ if((e.ctrlKey||e.metaKey) && e.key.toLowerCase()==="s"){ e.preventDefault(); save(); } }; window.addEventListener("keydown", onKey); return ()=>window.removeEventListener("keydown", onKey); },[value]);

  const fallback = (
    <Card className="bg-zinc-900/60 border-zinc-800">
      <CardHeader className="flex items-center justify-between">
        <CardTitle>Code (AssemblyScript) — fallback</CardTitle>
        <Button onClick={save}><Save className="w-4 h-4 mr-1"/>Save</Button>
      </CardHeader>
      <CardContent>
        <Textarea className="h-[60vh] bg-zinc-950 border-zinc-800 font-mono" value={value} onChange={(e)=>setValue(e.target.value)} />
      </CardContent>
    </Card>
  );

  return (
    <ErrorBoundary fallback={fallback}>
      <Card className="bg-zinc-900/60 border-zinc-800 flex flex-col min-h-[60vh]">
        <CardHeader className="flex flex-row items-center justify-between">
          <CardTitle>Code (AssemblyScript)</CardTitle>
          <div className="flex items-center gap-2">
            <Button variant="outline" className="border-zinc-700" onClick={format}>Format</Button>
            <Button onClick={save}><Save className="w-4 h-4 mr-1"/>Save</Button>
          </div>
        </CardHeader>
        <CardContent className="flex-1 min-h-0">
          <div className="h-[60vh] lg:h-[70vh] min-h-0">
            <Suspense fallback={<div className="text-sm text-zinc-400">Loading editor…</div>}>
              <MonacoEditor
                height="100%" defaultLanguage="typescript" theme="vs-dark"
                value={value} onChange={(v)=>setValue(v ?? "")} onMount={onMount}
                options={{ fontLigatures:true, fontSize:14, minimap:{ enabled:false }, smoothScrolling:true, scrollBeyondLastLine:false, automaticLayout:true, wordWrap:"on", tabSize:2 }}
              />
            </Suspense>
          </div>
        </CardContent>
      </Card>
    </ErrorBoundary>
  );
}
