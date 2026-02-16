import { motion } from 'framer-motion'
import { Cpu, Zap, Github } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion'
import { CodeBlock } from '@/components/code/code-block'
import { crates } from '@/data/crates'
import { Badge } from '@/components/ui/badge'
import { Link } from 'react-router-dom'

const crate = crates.maniac

export function ManiacPage() {
  return (
    <div className="min-h-screen">
      {/* Hero */}
      <motion.section
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.6 }}
        className="container mx-auto px-4 pt-32 pb-20"
      >
        <div className="max-w-4xl">
          <div className="flex items-center space-x-3 mb-4">
            <div className="p-3 bg-primary/10 rounded-lg">
              <Cpu className="h-8 w-8 text-primary" />
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

      {/* Key Features */}
      <section className="container mx-auto px-4 py-20">
        <div className="max-w-4xl">
          <h2 className="text-4xl font-bold mb-8">Key Features</h2>
          <Accordion type="single" collapsible className="w-full">
            <AccordionItem value="m-n">
              <AccordionTrigger className="text-lg">M:N Threading</AccordionTrigger>
              <AccordionContent>
                <p className="text-muted-foreground mb-4">
                  Maps M green threads onto N OS threads for maximum efficiency. Unlike 1:1 threading models,
                  maniac can spawn thousands of goroutines without OS thread overhead.
                </p>
                <ul className="list-disc list-inside text-muted-foreground space-y-2">
                  <li>Lightweight green threads (~8KB each)</li>
                  <li>Efficient load balancing via work stealing</li>
                  <li>No thread limits like OS threads</li>
                </ul>
              </AccordionContent>
            </AccordionItem>

            <AccordionItem value="stackful-coroutines">
              <AccordionTrigger className="text-lg">Stackful Coroutines</AccordionTrigger>
              <AccordionContent>
                <p className="text-muted-foreground">
                  Full stack support for async operations, enabling more natural control flow patterns
                  compared to stackless approaches like async/await.
                </p>
              </AccordionContent>
            </AccordionItem>

            <AccordionItem value="preemptive">
              <AccordionTrigger className="text-lg">Preemptive Scheduling</AccordionTrigger>
              <AccordionContent>
                <p className="text-muted-foreground">
                  Hybrid cooperative and preemptive scheduling ensures no single task can monopolize
                  CPU time, providing predictable performance under load.
                </p>
              </AccordionContent>
            </AccordionItem>

            <AccordionItem value="zero-cost">
              <AccordionTrigger className="text-lg">Zero-cost Abstractions</AccordionTrigger>
              <AccordionContent>
                <p className="text-muted-foreground">
                  Minimal runtime overhead with Rust's zero-cost abstraction principle. Compile-time
                  optimizations eliminate most runtime penalties.
                </p>
              </AccordionContent>
            </AccordionItem>
          </Accordion>
        </div>
      </section>

      {/* Quick Start */}
      <section className="container mx-auto px-4 py-20 bg-muted/50">
        <div className="max-w-4xl">
          <h2 className="text-4xl font-bold mb-8">Quick Start</h2>
          <CodeBlock code={crate.quickStart} />
        </div>
      </section>

      {/* Examples */}
      <section className="container mx-auto px-4 py-20">
        <div className="max-w-4xl">
          <h2 className="text-4xl font-bold mb-8">Examples</h2>
          <Tabs defaultValue={crate.examples[0].title} className="w-full">
            <TabsList className="grid w-full grid-cols-2">
              {crate.examples.map((example, index) => (
                <TabsTrigger key={index} value={example.title}>
                  {example.title}
                </TabsTrigger>
              ))}
            </TabsList>
            {crate.examples.map((example, index) => (
              <TabsContent key={index} value={example.title} className="mt-6">
                {example.description && (
                  <p className="text-muted-foreground mb-4">{example.description}</p>
                )}
                <CodeBlock code={example.code} />
              </TabsContent>
            ))}
          </Tabs>
        </div>
      </section>

      {/* Performance */}
      {crate.performance && (
        <section className="container mx-auto px-4 py-20 bg-muted/50">
          <div className="max-w-4xl">
            <h2 className="text-4xl font-bold mb-8">Performance</h2>
            <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
              {crate.performance.map((stat, index) => (
                <motion.div
                  key={index}
                  initial={{ opacity: 0, scale: 0.9 }}
                  whileInView={{ opacity: 1, scale: 1 }}
                  viewport={{ once: true }}
                  className="p-6 rounded-lg border bg-card text-center"
                >
                  <Zap className="h-8 w-8 text-primary mx-auto mb-4" />
                  <div className="text-3xl font-bold mb-2">{stat.value}</div>
                  <div className="text-sm text-muted-foreground mb-1">{stat.label}</div>
                  {stat.comparison && (
                    <div className="text-xs text-primary">{stat.comparison}</div>
                  )}
                </motion.div>
              ))}
            </div>
          </div>
        </section>
      )}
    </div>
  )
}
