# BOP Ecosystem Website

A beautiful, sleek, and bold static website showcasing the BOP (Bisque Orchestration Platform) ecosystem - a collection of high-performance Rust libraries for distributed systems.

## 📦 Project Overview

Built with React, TypeScript, Tailwind CSS, and Vite, this website is a client-side only static site showcasing three core Rust crates:
- **maniac** - High-performance async runtime with M:N threading
- **maniac-libsql** - Distributed SQLite with Git-like branching
- **maniac-raft** - Raft consensus adapter for OpenRaft

## 🚀 Tech Stack

- **Framework**: Vite + React 18.3 + TypeScript 5
- **Styling**: Tailwind CSS 3.4
- **UI Components**: shadcn/ui (Radix UI primitives)
- **Code Highlighting**: Prism.js
- **Routing**: react-router-dom 6
- **Animations**: framer-motion
- **Icons**: lucide-react

## 🏗️ Project Structure

```
web/
├── src/
│   ├── components/
│   │   ├── ui/              # shadcn/ui components
│   │   ├── layout/          # Navbar, Footer
│   │   ├── crate/           # CrateCard component
│   │   ├── code/            # Code blocks with syntax highlighting
│   │   └── marketing/       # Hero and feature components
│   ├── pages/               # Route components
│   │   ├── Homepage.tsx
│   │   ├── ManiacPage.tsx
│   │   ├── LibsqlPage.tsx
│   │   ├── RaftPage.tsx
│   │   ├── ArchitecturePage.tsx
│   │   └── ContributingPage.tsx
│   ├── data/                # Static data for crates
│   ├── hooks/               # Custom React hooks
│   ├── lib/                 # Utilities
│   ├── styles/              # Global styles and CSS
│   ├── App.tsx
│   └── main.tsx
├── public/                  # Static assets
├── package.json
├── tailwind.config.cjs
├── tsconfig.json
└── vite.config.ts
```

## 🛠️ Getting Started

### Prerequisites

- Node.js 18+ and npm

### Installation

```bash
cd web
npm install
```

### Development

```bash
npm run dev
```

The site will be available at `http://localhost:3000`

### Build for Production

```bash
npm run build
```

This creates an optimized production build in the `dist/` directory.

### Preview Production Build

```bash
npm run preview
```

## ✨ Features

### Implemented

✅ **Responsive Design** - Mobile-first approach, works on all screen sizes
✅ **Dark/Light Theme** - Toggle between dark and light mode with persistence
✅ **Code Syntax Highlighting** - Prism.js integration for Rust code
✅ **Copy to Clipboard** - One-click code copying
✅ **Smooth Animations** - framer-motion for transitions
✅ **Type-Safe** - Full TypeScript coverage
✅ **Client-Side Only** - No server components required
✅ **Static Build** - Optimized for static hosting

### Pages

1. **Homepage** (`/`)
   - Hero section with animated gradient
   - Three crate cards with hover effects
   - Feature highlights (Lightning Fast, Git-like Branching, Battle-Tested)
   - Quick start guide with tabbed code examples
   - CTA section

2. **maniac Page** (`/maniac`)
   - Detailed feature descriptions (accordion)
   - Quick start code example
   - Multiple code examples (HTTP Server, Timer Operations)
   - Performance statistics

3. **libsql Page** (`/libsql`)
   - Git-like branching explanation
   - MVCC versioning details
   - Zero-downtime migrations guide
   - Performance benchmarks

4. **raft Page** (`/raft`)
   - OpenRaft integration details
   - M:N threading integration
   - Multi-raft support explanation
   - Performance metrics

5. **Architecture Page** (`/architecture`)
   - Interactive SVG system diagram
   - How crates work together
   - Integration overview

6. **Contributing Page** (`/contributing`)
   - Development setup guide
   - Ways to contribute
   - Code style guidelines
   - PR process
   - Community links

### Design System

#### Color Palette (Monochromatic, Dark mode default)

```css
--background: #050505
--foreground: #fafafa
--primary: #b4b4b4
--accent: #8c8c8c
--muted: #1e1e1e
--border: #323232
```

The website uses a sleek, monochromatic grayscale palette that works in both light and dark modes.

#### Typography

- **Headings**: Plus Jakarta Sans (Bold: 700, Semibold: 600)
- **Body**: Plus Jakarta Sans (Regular: 400)
- **Monospace**: JetBrains Mono

#### Design Principles

- **Sleek**: Minimal, monochromatic design
- **Elegant**: Subtle grayscale variations for hierarchy
- **Modern**: Smooth animations, micro-interactions
- **Accessible**: WCAG AA compliant contrast

## 🎨 Customization

### Updating Crate Information

Edit `src/data/crates.ts` to modify:

- Crate descriptions
- Features list
- Quick start examples
- Performance metrics
- Code examples

### Modifying Styling

1. **Tailwind Config**: Edit `tailwind.config.cjs`
2. **Global Styles**: Edit `src/styles/globals.css`
3. **CSS Variables**: Modify color palette in `globals.css`

### Adding New Pages

1. Create page in `src/pages/`
2. Add route in `src/App.tsx`
3. Add navigation link in `src/data/crates.ts` (navigation array)

## 📦 Deployment

### Static Hosting

The `dist/` folder contains all static assets and can be deployed to:

- Netlify
- Vercel
- GitHub Pages
- Cloudflare Pages
- Any static file hosting

### Example: Deploy to Netlify

```bash
npm run build
# Drag and drop dist/ folder to Netlify dashboard
```

### Example: Deploy to Vercel

```bash
npm run build
vercel deploy --prod
```

## 🐛 Troubleshooting

### Build Errors

If you encounter build errors:

```bash
rm -rf node_modules dist
npm install
npm run build
```

### Type Errors

```bash
npm run type-check
```

This will show all TypeScript errors.

## 📝 License

MIT OR Apache-2.0

## 🤝 Contributing

Contributions welcome! See the [Contributing page](./contributing) for guidelines.

## 🔗 Links

- **GitHub**: https://github.com/bisque-io/bop
- **Documentation**: https://docs.bisque.io
- **Discord**: https://discord.gg/DUMMY

---

Built with ❤️ using React, TypeScript, and Tailwind CSS
