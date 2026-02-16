import { useState, useEffect } from 'react'
import { Search, FileCode, Database, Layers, BookOpen, Users, Globe } from 'lucide-react'
import { cn } from '@/lib/utils'
import Fuse from 'fuse.js'

interface SearchResult {
  title: string
  description: string
  path: string
  icon: string
  keywords?: string[]
}

const searchIndex: SearchResult[] = [
  {
    title: 'maniac-runtime',
    description: 'High-performance async runtime with M:N threading and stackful coroutines',
    path: '/maniac',
    icon: 'cpu',
    keywords: ['async', 'runtime', 'threading', 'coroutines', 'M:N', 'performance'],
  },
  {
    title: 'maniac-libsql',
    description: 'Distributed SQLite with Git-like branching and MVCC versioning',
    path: '/libsql',
    icon: 'database',
    keywords: ['sqlite', 'database', 'branching', 'git', 'mvcc', 'schema', 'migration'],
  },
  {
    title: 'maniac-raft',
    description: 'OpenRaft adapter for Maniac runtime with consensus and multi-raft support',
    path: '/raft',
    icon: 'layers',
    keywords: ['raft', 'consensus', 'openraft', 'multi-raft', 'distributed', 'replication'],
  },
  {
    title: 'Architecture',
    description: 'How all crates work together in the BOP ecosystem',
    path: '/architecture',
    icon: 'globe',
    keywords: ['architecture', 'design', 'integration', 'stack', 'ecosystem'],
  },
  {
    title: 'Contributing',
    description: 'Join us in building the future of high-performance distributed systems',
    path: '/contributing',
    icon: 'users',
    keywords: ['contributing', 'community', 'development', 'pull request', 'github', 'discord'],
  },
  {
    title: 'Documentation',
    description: 'Official documentation and guides for all crates',
    path: '/docs',
    icon: 'book-open',
    keywords: ['docs', 'documentation', 'guide', 'tutorial', 'examples'],
  },
]

const iconMap = {
  cpu: FileCode,
  database: Database,
  layers: Layers,
  globe: Globe,
  users: Users,
  'book-open': BookOpen,
}

interface SearchDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function SearchDialog({ open, onOpenChange }: SearchDialogProps) {
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<SearchResult[]>([])
  const [fuse] = useState(() => {
    return new Fuse(searchIndex, {
      keys: ['title', 'description', 'keywords'],
      threshold: 0.4,
      includeScore: true,
    })
  })

  useEffect(() => {
    if (query.trim()) {
      const fuseResults = fuse.search(query)
      setResults(fuseResults.map((r) => r.item))
    } else {
      setResults(searchIndex)
    }
  }, [query, fuse])

  useEffect(() => {
    if (open) {
      setQuery('')
      setResults(searchIndex)
    }
  }, [open])

  const IconWrapper = ({ name, className }: { name: string; className?: string }) => {
    const Icon = iconMap[name as keyof typeof iconMap] || FileCode
    return <Icon className={className} />
  }

  return (
    <>
      {open && (
        <div className="fixed inset-0 bg-background/80 backdrop-blur-sm z-50" onClick={() => onOpenChange(false)} />
      )}
      <div
        className={cn(
          'fixed left-[50%] top-[20%] z-50 w-full max-w-2xl -translate-x-1/2 transition-all duration-200',
          open ? 'scale-100 opacity-100' : 'scale-95 opacity-0 pointer-events-none'
        )}
      >
        <div className="border bg-background shadow-lg rounded-xl overflow-hidden">
          <div className="flex items-center border-b px-4">
            <Search className="h-4 w-4 text-muted-foreground" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search documentation, crates, features..."
              className="flex h-14 w-full bg-transparent px-4 py-3 text-lg outline-none placeholder:text-muted-foreground"
              autoFocus
            />
            <kbd
              className="pointer-events-none ml-4 inline-flex h-8 select-none items-center gap-1 rounded border bg-muted px-2 font-sans text-xs font-medium opacity-50"
            >
              <span className="text-xs">ESC</span>
            </kbd>
          </div>
          <div className="max-h-[400px] overflow-y-auto p-2">
            {results.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-12 text-center">
                <Search className="h-12 w-12 text-muted-foreground/50 mb-4" />
                <p className="text-sm text-muted-foreground">No results found</p>
                <p className="text-xs text-muted-foreground mt-1">
                  Try searching for crates, features, or documentation
                </p>
              </div>
            ) : (
              <div className="text-sm">
                {query.trim() && (
                  <div className="px-2 py-1.5 text-xs font-semibold text-muted-foreground">
                    {results.length} result{results.length !== 1 ? 's' : ''} found
                  </div>
                )}
                {results.map((result, index) => (
                  <a
                    key={index}
                    href={result.path}
                    onClick={(e) => {
                      e.preventDefault()
                      onOpenChange(false)
                      window.location.href = result.path
                    }}
                    className="flex items-center gap-3 rounded-lg px-3 py-3 hover:bg-accent hover:text-accent-foreground transition-colors"
                  >
                    <IconWrapper name={result.icon} className="h-5 w-5 text-muted-foreground" />
                    <div className="flex-1">
                      <div className="font-medium">{result.title}</div>
                      <div className="text-xs text-muted-foreground mt-0.5">
                        {result.description}
                      </div>
                    </div>
                    <kbd className="pointer-events-none inline-flex h-5 select-none items-center gap-1 rounded border bg-muted px-1.5 font-mono text-xs">
                      ↵
                    </kbd>
                  </a>
                ))}
              </div>
            )}
          </div>
          <div className="border-t px-4 py-2 flex items-center justify-between text-xs text-muted-foreground">
            <div className="flex items-center gap-4">
              <span>Navigation</span>
              <div className="flex items-center gap-1">
                <kbd className="inline-flex h-4 items-center rounded border bg-muted px-1.5 font-sans">
                  ↑↓
                </kbd>
                <span>to navigate</span>
              </div>
              <div className="flex items-center gap-1">
                <kbd className="inline-flex h-4 items-center rounded border bg-muted px-1.5 font-sans">
                  ↵
                </kbd>
                <span>to select</span>
              </div>
            </div>
            <div className="flex items-center gap-1">
              <kbd className="inline-flex h-4 items-center rounded border bg-muted px-1.5 font-sans">
                ESC
              </kbd>
              <span>to close</span>
            </div>
          </div>
        </div>
      </div>
    </>
  )
}
