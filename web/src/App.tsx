import { BrowserRouter as Router, Routes, Route } from 'react-router-dom'
import { ThemeProvider } from '@/hooks/use-theme'
import { Navbar } from '@/components/layout/navbar'
import { Footer } from '@/components/layout/footer'
import { Homepage } from '@/pages/Homepage'
import { ManiacPage } from '@/pages/ManiacPage'
import { LibsqlPage } from '@/pages/LibsqlPage'
import { RaftPage } from '@/pages/RaftPage'
import { ArchitecturePage } from '@/pages/ArchitecturePage'
import { ContributingPage } from '@/pages/ContributingPage'

function App() {
  return (
    <ThemeProvider defaultTheme="dark" storageKey="bop-theme">
      <Router>
        <div className="min-h-screen flex flex-col bg-background">
          <Navbar />
          <main className="flex-1">
            <Routes>
              <Route path="/" element={<Homepage />} />
              <Route path="/maniac" element={<ManiacPage />} />
              <Route path="/libsql" element={<LibsqlPage />} />
              <Route path="/raft" element={<RaftPage />} />
              <Route path="/architecture" element={<ArchitecturePage />} />
              <Route path="/contributing" element={<ContributingPage />} />
            </Routes>
          </main>
          <Footer />
        </div>
      </Router>
    </ThemeProvider>
  )
}

export default App
