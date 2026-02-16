
import { motion } from 'framer-motion'
import { Github, BookOpen, MessageSquare, Code2, Bug, Lightbulb } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from '@/components/ui/accordion'
import { resources } from '@/data/crates'
import { CodeBlock } from '@/components/code/code-block'

export function ContributingPage() {
  return (
    <div className="min-h-screen">
      <motion.section
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.6 }}
        className="container mx-auto px-4 pt-32 pb-20"
      >
        <div className="text-center max-w-4xl mx-auto">
          <h1 className="text-5xl md:text-6xl font-bold mb-6">Contributing</h1>
          <p className="text-xl text-muted-foreground mb-8">
            Join us in building the future of high-performance distributed systems
          </p>
          <div className="flex flex-wrap justify-center gap-4">
            <Button size="lg" asChild>
              <a href={resources.github} target="_blank" rel="noopener noreferrer">
                <Github className="mr-2 h-5 w-5" />
                View on GitHub
              </a>
            </Button>
            <Button size="lg" variant="outline" asChild>
              <a href={resources.discord} target="_blank" rel="noopener noreferrer">
                <MessageSquare className="mr-2 h-5 w-5" />
                Join Discord
              </a>
            </Button>
          </div>
        </div>
      </motion.section>

      {/* Get Started */}
      <section className="container mx-auto px-4 py-20 bg-muted/50">
        <div className="max-w-4xl">
          <h2 className="text-4xl font-bold mb-8">Get Started</h2>
          <div className="space-y-6">
            <Card>
              <CardHeader>
                <CardTitle className="flex items-center gap-2">
                  <Code2 className="h-5 w-5" />
                  Prerequisites
                </CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                <div>
                  <h4 className="font-semibold mb-2">1. Install Rust</h4>
                  <CodeBlock
                    code={`# Install Rust 2024 Edition
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install nightly
rustup default nightly`}
                  />
                </div>
                <div>
                  <h4 className="font-semibold mb-2">2. Clone the Repository</h4>
                  <CodeBlock
                    code={`git clone https://github.com/bisque-io/bop.git
cd bop`}
                  />
                </div>
                <div>
                  <h4 className="font-semibold mb-2">3. Build the Project</h4>
                  <CodeBlock
                    code={`# Build all crates
cargo build --workspace

# Build specific crate
cargo build -p maniac

# Run tests
cargo test --workspace`}
                  />
                </div>
              </CardContent>
            </Card>
          </div>
        </div>
      </section>

      {/* Ways to Contribute */}
      <section className="container mx-auto px-4 py-20">
        <div className="max-w-6xl">
          <h2 className="text-4xl font-bold mb-8">Ways to Contribute</h2>
          <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
            <Card>
              <CardHeader>
                <Bug className="h-10 w-10 mb-2 text-foreground" />
                <CardTitle>Report Bugs</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>
                 _found issues or unexpected behavior? Open an issue with detailed steps to reproduce.
                </CardDescription>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <Lightbulb className="h-10 w-10 mb-2 text-foreground" />
                <CardTitle>Feature Requests</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>
                  Have an idea for improvement? Open a feature request and explain the use case.
                </CardDescription>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <Code2 className="h-10 w-10 mb-2 text-foreground" />
                <CardTitle>Pull Requests</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>
                  Fix bugs, add features, improve documentation. All contributions are welcome.
                </CardDescription>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <BookOpen className="h-10 w-10 mb-2 text-foreground" />
                <CardTitle>Documentation</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>
                  Improve docs, add examples, write tutorials. Share what you know.
                </CardDescription>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <Github className="h-10 w-10 mb-2 text-foreground" />
                <CardTitle>Review PRs</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>
                  Help review pull requests, provide feedback, ensure code quality.
                </CardDescription>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <MessageSquare className="h-10 w-10 mb-2 text-foreground" />
                <CardTitle>Community Support</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>
                  Answer questions, help others, share knowledge on Discord and GitHub.
                </CardDescription>
              </CardContent>
            </Card>
          </div>
        </div>
      </section>

      {/* Development Guidelines */}
      <section className="container mx-auto px-4 py-20 bg-muted/50">
        <div className="max-w-4xl">
          <h2 className="text-4xl font-bold mb-8">Development Guidelines</h2>
          <Accordion type="single" collapsible className="w-full">
            <AccordionItem value="code-style">
              <AccordionTrigger className="text-lg">Code Style</AccordionTrigger>
              <AccordionContent>
                <ul className="list-disc list-inside text-muted-foreground space-y-2">
                  <li>Follow Rust standard conventions (rustfmt)</li>
                  <li>Use clippy for linting</li>
                  <li>Write descriptive commit messages</li>
                  <li>Add tests for new features</li>
                  <li>Document public APIs</li>
                </ul>
              </AccordionContent>
            </AccordionItem>

            <AccordionItem value="testing">
              <AccordionTrigger className="text-lg">Testing</AccordionTrigger>
              <AccordionContent>
                <ul className="list-disc list-inside text-muted-foreground space-y-2">
                  <li>Write unit tests for new functionality</li>
                  <li>Use integration tests for complex scenarios</li>
                  <li>Ensure all tests pass before submitting</li>
                  <li>Consider adding benchmarks for performance-sensitive code</li>
                </ul>
              </AccordionContent>
            </AccordionItem>

            <AccordionItem value="commits">
              <AccordionTrigger className="text-lg">Commit Messages</AccordionTrigger>
              <AccordionContent>
                <p className="text-muted-foreground mb-4">
                  Follow conventional commits format:
                </p>
                <CodeBlock
                  code={`feat: add support for async file I/O
fix: resolve race condition in work stealing
docs: update README with quick start guide
refactor: simplify task scheduling logic
test: add integration tests for multi-raft cluster`}
                />
              </AccordionContent>
            </AccordionItem>

            <AccordionItem value="pr-process">
              <AccordionTrigger className="text-lg">PR Process</AccordionTrigger>
              <AccordionContent>
                <ol className="list-decimal list-inside text-muted-foreground space-y-2">
                  <li>Fork the repository</li>
                  <li>Create a branch for your change</li>
                  <li>Make your changes and add tests</li>
                  <li>Ensure all tests pass</li>
                  <li>Submit a pull request with clear description</li>
                  <li>Address review feedback</li>
                  <li>Wait for maintainer approval and merge</li>
                </ol>
              </AccordionContent>
            </AccordionItem>
          </Accordion>
        </div>
      </section>

      {/* Code of Conduct */}
      <section className="container mx-auto px-4 py-20">
        <div className="max-w-4xl">
          <Card className="border-accent">
            <CardHeader>
              <CardTitle>Code of Conduct</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground mb-4">
                We are committed to providing a welcoming and inclusive environment. Please:
              </p>
              <ul className="list-disc list-inside text-muted-foreground space-y-2">
                <li>Be respectful and considerate</li>
                <li>Use welcoming and inclusive language</li>
                <li>Focus on constructive discussions</li>
                <li>Gracefully accept constructive criticism</li>
                <li>Show empathy towards community members</li>
              </ul>
            </CardContent>
          </Card>
        </div>
      </section>

      {/* Community */}
      <section className="container mx-auto px-4 py-20 bg-muted/50">
        <div className="max-w-4xl">
          <h2 className="text-4xl font-bold mb-8">Join the Community</h2>
          <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
            <Card>
              <CardHeader>
                <Github className="h-10 w-10 mb-2" />
                <CardTitle>GitHub</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>Fork, star, and contribute to the repository.</CardDescription>
                <Button variant="link" className="mt-4 p-0" asChild>
                  <a href={resources.github} target="_blank" rel="noopener noreferrer">
                    View Repository →
                  </a>
                </Button>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <MessageSquare className="h-10 w-10 mb-2" />
                <CardTitle>Discord</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>Chat with maintainers and community members.</CardDescription>
                <Button variant="link" className="mt-4 p-0" asChild>
                  <a href={resources.discord} target="_blank" rel="noopener noreferrer">
                    Join Server →
                  </a>
                </Button>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <BookOpen className="h-10 w-10 mb-2" />
                <CardTitle>Documentation</CardTitle>
              </CardHeader>
              <CardContent>
                <CardDescription>Read the docs and explore examples.</CardDescription>
                <Button variant="link" className="mt-4 p-0" asChild>
                  <a href={resources.docs} target="_blank" rel="noopener noreferrer">
                    Read Docs →
                  </a>
                </Button>
              </CardContent>
            </Card>
          </div>
        </div>
      </section>
    </div>
  )
}
