import { motion } from 'framer-motion'
import { ArrowDown } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Link } from 'react-router-dom'

export function ArchitecturePage() {
  return (
    <div className="min-h-screen">
      <motion.section
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.6 }}
        className="container mx-auto px-4 pt-32 pb-20"
      >
        <div className="text-center max-w-4xl mx-auto">
          <h1 className="text-5xl md:text-6xl font-bold mb-6">Architecture</h1>
          <p className="text-xl text-muted-foreground mb-8">
            How maniac, libsql, and raft work together to build high-performance distributed systems
          </p>
        </div>
      </motion.section>

      {/* System Diagram */}
      <section className="container mx-auto px-4 py-20">
        <div className="max-w-6xl mx-auto">
          <Card className="p-12">
            <svg viewBox="0 0 800 600" className="w-full h-auto">
              {}
              <defs>
                <marker id="arrowhead" markerWidth="10" markerHeight="7" refX="9" refY="3.5" orient="auto">
                  <polygon points="0 0, 10 3.5, 0 7" fill="currentColor" className="fill-foreground" />
                </marker>
              </defs>

              {}
              <text x="400" y="40" textAnchor="middle" className="text-2xl font-bold fill-foreground">
                Client Applications
              </text>

              {}
              <rect x="250" y="60" width="300" height="80" rx="8" className="fill-muted/50 stroke-border" strokeWidth="2" />
              <text x="400" y="100" textAnchor="middle" className="fill-foreground">Java, Go, Rust, Python</text>
              <text x="400" y="120" textAnchor="middle" className="fill-muted-foreground text-sm">Connect via libbop or bopd</text>

              {}
              <line x1="400" y1="140" x2="400" y2="170" stroke="currentColor" className="stroke-foreground" strokeWidth="2" markerEnd="url(#arrowhead)" />

              {}
              <rect x="200" y="180" width="400" height="100" rx="8" className="fill-muted/40 stroke-border" strokeWidth="2" />
              <text x="400" y="220" textAnchor="middle" className="text-xl font-semibold fill-foreground">maniac-raft</text>
              <text x="400" y="245" textAnchor="middle" className="fill-muted-foreground text-sm">OpenRaft + Maniac Runtime</text>
              <text x="400" y="265" textAnchor="middle" className="fill-muted-foreground text-sm">Consensus & Replication</text>

              {}
              <line x1="400" y1="280" x2="400" y2="310" stroke="currentColor" className="stroke-foreground" strokeWidth="2" markerEnd="url(#arrowhead)" />

              {}
              <rect x="200" y="320" width="400" height="100" rx="8" className="fill-muted/30 stroke-border" strokeWidth="2" />
              <text x="400" y="360" textAnchor="middle" className="text-xl font-semibold fill-foreground">maniac-libsql</text>
              <text x="400" y="385" textAnchor="middle" className="fill-muted-foreground text-sm">Distributed SQLite with Branching</text>
              <text x="400" y="405" textAnchor="middle" className="fill-muted-foreground text-sm">Raft State Machine & Storage</text>

              {}
              <line x1="400" y1="420" x2="400" y2="450" stroke="currentColor" className="stroke-foreground" strokeWidth="2" markerEnd="url(#arrowhead)" />

              {}
              <rect x="200" y="460" width="400" height="100" rx="8" className="fill-muted/20 stroke-border" strokeWidth="2" />
              <text x="400" y="500" textAnchor="middle" className="text-xl font-semibold fill-foreground">maniac-runtime</text>
              <text x="400" y="525" textAnchor="middle" className="fill-muted-foreground text-sm">M:N Threading + Stackful Coroutines</text>
              <text x="400" y="545" textAnchor="middle" className="fill-muted-foreground text-sm">Async Foundation</text>

              {}
              <rect x="620" y="320" width="150" height="240" rx="8" className="fill-muted/10 stroke-border" strokeWidth="2" />
              <text x="695" y="400" textAnchor="middle" className="font-semibold fill-foreground">External</text>
              <text x="695" y="420" textAnchor="middle" className="fill-muted-foreground text-sm">S3 Storage</text>
              <text x="695" y="440" textAnchor="middle" className="fill-muted-foreground text-sm">Network Layer</text>
              <text x="695" y="460" textAnchor="middle" className="fill-muted-foreground text-sm">Cryptographic</text>

              {}
              <line x1="600" y1="370" x2="620" y2="370" stroke="currentColor" className="stroke-muted-foreground" strokeWidth="2" strokeDasharray="5,5" />
            </svg>
          </Card>
        </div>
      </section>

      {/* How They Work Together */}
      <section className="container mx-auto px-4 py-20 bg-muted/50">
        <div className="max-w-4xl">
          <h2 className="text-4xl font-bold mb-8">How They Work Together</h2>
          <div className="space-y-6">
            <Card>
              <CardHeader>
                <CardTitle>1. Foundation: maniac-runtime</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>
                  Provides the async runtime foundation with M:N threading, stackful coroutines,
                  and high-performance I/O. All other crates build on this foundation.
                </CardDescription>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>2. Consensus: maniac-raft</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>
                  Implements OpenRaft's AsyncRuntime trait using maniac's primitives.
                  Provides strong consistency and fault tolerance for distributed state machines.
                </CardDescription>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>3. Storage: maniac-libsql</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>
                  Implements a Raft state machine as a SQLite database with git-like branching.
                  Uses Raft for consensus while providing advanced features like schema evolution.
                </CardDescription>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>4. Integration</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>
                  Together, these crates form a complete stack for building distributed systems:
                  async runtime → consensus → storage. Each crate can be used independently or together.
                </CardDescription>
              </CardContent>
            </Card>
          </div>
        </div>
      </section>

      {/* CTA */}
      <section className="container mx-auto px-4 py-20">
        <div className="max-w-4xl mx-auto text-center">
          <h2 className="text-4xl font-bold mb-6">Ready to Build?</h2>
          <p className="text-xl text-muted-foreground mb-8">
            Start building high-performance distributed systems today
          </p>
          <Button size="lg" asChild>
            <Link to="/">
              <ArrowDown className="mr-2 h-5 w-5" />
              View All Crates
            </Link>
          </Button>
        </div>
      </section>
    </div>
  )
}
