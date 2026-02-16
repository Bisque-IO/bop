import { motion } from 'framer-motion'
import { Layers, Github } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion'
import { CodeBlock } from '@/components/code/code-block'
import { crates } from '@/data/crates'
import { Badge } from '@/components/ui/badge'
import { Link } from 'react-router-dom'

const crate = crates.raft

export function RaftPage() {
  return (
    <div className="min-h-screen">
      <motion.section initial={{ opacity: 0, y: 20 }} animate={{ opacity: 1, y: 0 }} transition={{ duration: 0.6 }} className="container mx-auto px-4 pt-32 pb-20">
        <div className="max-w-4xl">
          <div className="flex items-center space-x-3 mb-4">
            <div className="p-3 bg-primary/10 rounded-lg">
              <Layers className="h-8 w-8 text-primary" />
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

      <section className="container mx-auto px-4 py-20">
        <div className="max-w-4xl">
          <h2 className="text-4xl font-bold mb-8">Key Features</h2>
          <Accordion type="single" collapsible className="w-full">
            <AccordionItem value="openraft">
              <AccordionTrigger className="text-lg">OpenRaft Integration</AccordionTrigger>
              <AccordionContent>
                <p className="text-muted-foreground mb-4">
                  Full AsyncRuntime trait implementation for OpenRaft, enabling consensus on top of
                  Maniac's high-performance async primitives.
                </p>
                <ul className="list-disc list-inside text-muted-foreground space-y-2">
                  <li>Complete AsyncRuntime trait implementation</li>
                  <li>Compatible with OpenRaft's type system</li>
                  <li>Type-safe networking and storage</li>
                </ul>
              </AccordionContent>
            </AccordionItem>

            <AccordionItem value="runtime">
              <AccordionTrigger className="text-lg">Maniac Runtime</AccordionTrigger>
              <AccordionContent>
                <p className="text-muted-foreground">
                  Uses Maniac's async primitives for maximum performance. Benefits from M:N threading,
                  work stealing, and zero-cost abstractions.
                </p>
              </AccordionContent>
            </AccordionItem>

            <AccordionItem value="multi-raft">
              <AccordionTrigger className="text-lg">Multi-Raft Support</AccordionTrigger>
              <AccordionContent>
                <p className="text-muted-foreground">
                  Run multiple Raft groups simultaneously, each with isolated state machines and logs.
                  Perfect for sharded distributed systems.
                </p>
              </AccordionContent>
            </AccordionItem>

            <AccordionItem value="zero-cost">
              <AccordionTrigger className="text-lg">Zero-cost Abstractions</AccordionTrigger>
              <AccordionContent>
                <p className="text-muted-foreground">
                  Minimal wrapper overhead. Most operations compile down to direct calls to OpenRaft
                  without intermediate allocations.
                </p>
              </AccordionContent>
            </AccordionItem>
          </Accordion>
        </div>
      </section>

      <section className="container mx-auto px-4 py-20 bg-muted/50">
        <div className="max-w-4xl">
          <h2 className="text-4xl font-bold mb-8">Quick Start</h2>
          <CodeBlock code={crate.quickStart} />
        </div>
      </section>

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
            <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
              {crate.performance.map((stat, index) => (
                <motion.div key={index} initial={{ opacity: 0, scale: 0.9 }} whileInView={{ opacity: 1, scale: 1 }} viewport={{ once: true }} className="p-6 rounded-lg border bg-card text-center">
                  <Layers className="h-8 w-8 text-primary mx-auto mb-4" />
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
