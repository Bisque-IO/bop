import { motion } from 'framer-motion'
import { Database, Shield, Github } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion'
import { CodeBlock } from '@/components/code/code-block'
import { crates } from '@/data/crates'
import { Badge } from '@/components/ui/badge'
import { Link } from 'react-router-dom'

const crate = crates.libsql

export function LibsqlPage() {
  return (
    <div className="min-h-screen">
      {}
      <motion.section initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.6 }} className="container mx-auto px-4 pt-32 pb-20">
        <div className="max-w-4xl">
          <div className="flex items-center space-x-3 mb-4">
            <div className="p-3 bg-primary/10 rounded-lg">
              <Database className="h-8 w-8 text-primary" />
            </div>
            <Badge variant="secondary">v{crate.version}</Badge>
          </div>
          <h1 className="text-5xl md:text-6xl font-bold mb-6">{crate.name}</h1>
          <p className="text-2xl text-muted-foreground mb-8">{crate.tagline}</p>
          <p className="text-lg text-muted-foreground mb-8">{crate.description}</p>
          <div className="flex flex-wrap gap-4">
            <Button asChild>
              <a href="https://github.com/bisque-io/bop" target="_blank" rel="noopener noreferrer">
                <Github className="mr-2 h-5 w-5" />
                GitHub
              </a>
            </Button>
            <Button variant="outline" asChild>
              <Link to="/docs">Documentation</Link>
            </Button>
          </div>
        </div>
      </motion.section>

      {}
      <section className="container mx-auto px-4 py-20">
        <div className="max-w-4xl">
          <h2 className="text-4xl font-bold mb-8">Key Features</h2>
          <Accordion type="single" collapsible className="w-full">
            <AccordionItem value="branching">
              <AccordionTrigger className="text-lg">Git-like Branching</AccordionTrigger>
              <AccordionContent>
                <p className="text-muted-foreground mb-4">
                  Fork databases at any point in O(1) time. Create isolated branches for testing schema changes,
                  feature development, or multi-tenant isolation.
                </p>
                <ul className="list-disc list-inside text-muted-foreground space-y-2">
                  <li>Instant branching via metadata pointers</li>
                  <li>Copy-on-write semantics</li>
                  <li>Branch hierarchy support</li>
                </ul>
              </AccordionContent>
            </AccordionItem>

            <AccordionItem value="mvcc">
              <AccordionTrigger className="text-lg">MVCC Versioning</AccordionTrigger>
              <AccordionContent>
                <p className="text-muted-foreground">
                  Multiple versions of data coexist without conflicts. Readers never block writers,
                  and isolated transactions guarantee consistency.
                </p>
              </AccordionContent>
            </AccordionItem>

            <AccordionItem value="evolution">
              <AccordionTrigger className="text-lg">Zero-downtime Migrations</AccordionTrigger>
              <AccordionContent>
                <p className="text-muted-foreground">
                  Evolve schemas without service interruption. Use schema evolution to test migrations
                  in isolation, then cut over when ready.
                </p>
              </AccordionContent>
            </AccordionItem>

            <AccordionItem value="raft">
              <AccordionTrigger className="text-lg">Raft Consensus</AccordionTrigger>
              <AccordionContent>
                <p className="text-muted-foreground">
                  Built-in Raft consensus for fault tolerance and strong consistency. Automatic leader
                  election and log replication across multiple nodes.
                </p>
              </AccordionContent>
            </AccordionItem>
          </Accordion>
        </div>
      </section>

      {}
      <section className="container mx-auto px-4 py-20 bg-muted/50">
        <div className="max-w-4xl">
          <h2 className="text-4xl font-bold mb-8">Quick Start</h2>
          <CodeBlock code={crate.quickStart} />
        </div>
      </section>

      {}
      <section className="container mx-auto px-4 py-20">
        <div className="max-w-4xl">
          <h2 className="text-4xl font-bold mb-8">Examples</h2>
          <Tabs defaultValue={crate.examples[0].title} className="w-full">
            <TabsList className="grid w-full grid-cols-2">
              {crate.examples.map((example, index) => (
                <TabsTrigger key={index} value={example.title}>{example.title}</TabsTrigger>
              ))}
            </TabsList>
            {crate.examples.map((example, index) => (
              <TabsContent key={index} value={example.title} className="mt-6">
                {example.description && <p className="text-muted-foreground mb-4">{example.description}</p>}
                <CodeBlock code={example.code} />
              </TabsContent>
            ))}
          </Tabs>
        </div>
      </section>

      {crate.performance && (
        <section className="container mx-auto px-4 py-20 bg-muted/50">
          <div className="max-w-4xl">
            <h2 className="text-4xl font-bold mb-8">Performance</h2>
            <div className="grid grid-cols-1 md:grid-cols-4 gap-6">
              {crate.performance.map((stat, index) => (
                <motion.div key={index} initial={{ opacity: 0, scale: 0.9 }} whileInView={{ opacity: 1, scale: 1 }} viewport={{ once: true }} className="p-6 rounded-lg border bg-card text-center">
                  <Shield className="h-8 w-8 text-primary mx-auto mb-4" />
                  <div className="text-3xl font-bold mb-2">{stat.value}</div>
                  <div className="text-sm text-muted-foreground mb-1">{stat.label}</div>
                  {stat.comparison && <div className="text-xs text-primary">{stat.comparison}</div>}
                </motion.div>
              ))}
            </div>
          </div>
        </section>
      )}
    </div>
  )
}
