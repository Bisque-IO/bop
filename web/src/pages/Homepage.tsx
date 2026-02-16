
import { Link } from 'react-router-dom'
import { motion } from 'framer-motion'
import { ArrowRight, Zap, GitBranch, Shield, Github } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { CrateCard } from '@/components/crate/crate-card'
import { crates } from '@/data/crates'
import { CodeBlock } from '@/components/code/code-block'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Badge } from '@/components/ui/badge'

const containerVariants = {
  hidden: { opacity: 0 },
  visible: {
    opacity: 1,
    transition: {
      staggerChildren: 0.1,
    },
  },
}

const itemVariants = {
  hidden: { opacity: 0, y: 20 },
  visible: {
    opacity: 1,
    y: 0,
    transition: {
      duration: 0.5,
    },
  },
}

export function Homepage() {
  return (
    <div className="min-h-screen">
      {/* Hero Section */}
      <motion.section
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.6 }}
        className="container mx-auto px-4 pt-32 pb-20"
      >
        <div className="text-center max-w-4xl mx-auto">
          <Badge className="mb-4" variant="secondary">
            High-Performance Rust Ecosystem
          </Badge>
          <h1 className="text-5xl md:text-7xl font-bold mb-6">
            High-Performance Distributed Systems
          </h1>
          <p className="text-xl text-muted-foreground mb-8">
            M:N async runtime • Distributed SQLite with branching • Raft consensus
          </p>
          <div className="flex flex-col sm:flex-row gap-4 justify-center">
            <Button size="lg" asChild>
              <Link to="/maniac">
                Get Started
                <ArrowRight className="ml-2 h-5 w-5" />
              </Link>
            </Button>
            <Button size="lg" variant="outline" asChild>
              <Link to="/architecture">View Architecture</Link>
            </Button>
          </div>
        </div>
      </motion.section>

      {/* Features */}
      <motion.section
        variants={containerVariants}
        initial="hidden"
        whileInView="visible"
        viewport={{ once: true }}
        className="container mx-auto px-4 py-20"
      >
        <div className="grid grid-cols-1 md:grid-cols-3 gap-8">
          <motion.div variants={itemVariants} className="p-6 rounded-lg border bg-card">
            <div className="flex items-center space-x-3 mb-4">
              <div className="p-3 bg-muted rounded-lg">
                <Zap className="h-6 w-6 text-foreground" />
              </div>
              <h3 className="text-lg font-semibold">Lightning Fast</h3>
            </div>
            <p className="text-muted-foreground">
              Zero-cost abstractions with minimal runtime overhead. Optimized for throughput and low latency.
            </p>
          </motion.div>

          <motion.div variants={itemVariants} className="p-6 rounded-lg border bg-card">
            <div className="flex items-center space-x-3 mb-4">
              <div className="p-3 bg-muted rounded-lg">
                <GitBranch className="h-6 w-6 text-foreground" />
              </div>
              <h3 className="text-lg font-semibold">Git-like Branching</h3>
            </div>
            <p className="text-muted-foreground">
              Fork databases at any point in O(1). Test schema changes safely with zero downtime migrations.
            </p>
          </motion.div>

          <motion.div variants={itemVariants} className="p-6 rounded-lg border bg-card">
            <div className="flex items-center space-x-3 mb-4">
              <div className="p-3 bg-muted rounded-lg">
                <Shield className="h-6 w-6 text-foreground" />
              </div>
              <h3 className="text-lg font-semibold">Battle-Tested</h3>
            </div>
            <p className="text-muted-foreground">
              Raft consensus ensures strong consistency. Fault-tolerant with automatic leader election.
            </p>
          </motion.div>
        </div>
      </motion.section>

      {/* Crates */}
      <motion.section
        variants={containerVariants}
        initial="hidden"
        whileInView="visible"
        viewport={{ once: true }}
        className="container mx-auto px-4 py-20"
      >
        <div className="text-center mb-12">
          <h2 className="text-4xl font-bold mb-4">Core Crates</h2>
          <p className="text-xl text-muted-foreground">
            Three powerful libraries working together
          </p>
        </div>
        <div className="grid grid-cols-1 md:grid-cols-3 gap-8 max-w-6xl mx-auto">
          {Object.values(crates).map((crate) => (
            <CrateCard key={crate.id} {...crate} />
          ))}
        </div>
      </motion.section>

      {/* Quick Start */}
      <motion.section
        variants={containerVariants}
        initial="hidden"
        whileInView="visible"
        viewport={{ once: true }}
        className="container mx-auto px-4 py-20"
      >
        <div className="max-w-4xl mx-auto">
          <div className="text-center mb-12">
            <h2 className="text-4xl font-bold mb-4">Quick Start</h2>
            <p className="text-xl text-muted-foreground">
              Get up and running in minutes
            </p>
          </div>

          <Tabs defaultValue="maniac" className="w-full">
            <TabsList className="grid w-full grid-cols-3">
              <TabsTrigger value="maniac">maniac</TabsTrigger>
              <TabsTrigger value="libsql">libsql</TabsTrigger>
              <TabsTrigger value="raft">raft</TabsTrigger>
            </TabsList>

            <TabsContent value="maniac" className="mt-6">
              <CodeBlock
                code={crates.maniac.quickStart}
                language="rust"
              />
            </TabsContent>

            <TabsContent value="libsql" className="mt-6">
              <CodeBlock
                code={crates.libsql.quickStart}
                language="rust"
              />
            </TabsContent>

            <TabsContent value="raft" className="mt-6">
              <CodeBlock
                code={crates.raft.quickStart}
                language="rust"
              />
            </TabsContent>
          </Tabs>
        </div>
      </motion.section>

      {/* CTA */}
      <motion.section
        initial={{ opacity: 0 }}
        whileInView={{ opacity: 1 }}
        viewport={{ once: true }}
        className="container mx-auto px-4 py-20"
      >
        <div className="max-w-4xl mx-auto text-center p-12 rounded-2xl bg-muted border border-border">
          <h2 className="text-4xl font-bold mb-6">Ready to Build?</h2>
          <p className="text-xl text-muted-foreground mb-8">
            Start building high-performance distributed systems today
          </p>
          <div className="flex flex-col sm:flex-row gap-4 justify-center">
            <Button size="lg" asChild>
              <a href="https://github.com/bisque-io/bop" target="_blank" rel="noopener noreferrer">
                <Github className="mr-2 h-5 w-5" />
                View on GitHub
              </a>
            </Button>
            <Button size="lg" variant="outline" asChild>
              <Link to="/contributing">Contributing Guide</Link>
            </Button>
          </div>
        </div>
      </motion.section>
    </div>
  )
}
