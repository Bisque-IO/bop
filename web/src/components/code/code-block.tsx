import { useEffect, useRef, useState } from 'react'
import Prism from 'prismjs'
import 'prismjs/components/prism-rust'
import { Copy, Check } from 'lucide-react'
import { cn } from '@/lib/utils'

interface CodeBlockProps {
  code: string
  language?: string
  className?: string
  showCopyButton?: boolean
}

export function CodeBlock({
  code,
  language = 'rust',
  className,
  showCopyButton = true,
}: CodeBlockProps) {
  const codeRef = useRef<HTMLElement>(null)
  const [copied, setCopied] = useState(false)

  useEffect(() => {
    if (codeRef.current) {
      Prism.highlightElement(codeRef.current)
    }
  }, [code, language])

  const handleCopy = async () => {
    await navigator.clipboard.writeText(code)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  return (
    <div className={cn('relative group rounded-lg overflow-hidden', className)}>
      {showCopyButton && (
        <button
          onClick={handleCopy}
          className="absolute top-3 right-3 z-10 p-2 rounded-md bg-white/10 hover:bg-white/20 opacity-0 group-hover:opacity-100 transition-opacity"
          aria-label="Copy code"
        >
          {copied ? (
            <Check className="h-4 w-4 text-white" />
          ) : (
            <Copy className="h-4 w-4 text-white/70" />
          )}
        </button>
      )}
      <pre className={`language-${language}`}>
        <code ref={codeRef} className={`language-${language}`}>
          {code}
        </code>
      </pre>
    </div>
  )
}
