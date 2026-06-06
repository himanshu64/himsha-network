import { useState } from 'react'
import {
  Search,
  Github,
  Menu,
  X,
  ShieldCheck,
  Cpu,
  Bitcoin,
  Layers,
  Coins,
  KeyRound,
  ArrowRight,
  ChevronLeft,
  ChevronRight,
} from 'lucide-react'

const GITHUB = 'https://github.com/himanshu64/himsha-network'

const NAV = [
  { label: 'Programs', href: `${GITHUB}#programs` },
  { label: 'How It Works', href: `${GITHUB}#architecture` },
  { label: 'SDKs', href: `${GITHUB}/tree/main/himsha-sdk` },
  { label: 'Docs', href: `${GITHUB}/tree/main/docs` },
  { label: 'Roadmap', href: `${GITHUB}/milestone/1` },
]

// Hero slides the Previous / Next arrows cycle through.
const SLIDES = [
  {
    meta: [
      { Icon: ShieldCheck, text: '182 tests passing' },
      { Icon: Cpu, text: 'RISC Zero zkVM' },
      { Icon: Bitcoin, text: 'Settles on Bitcoin' },
    ],
    title: ['Program Bitcoin.', 'Proven by ZK.'],
    desc: 'An experimental Bitcoin programmability layer — every state transition is proven correct by a RISC Zero ZK receipt, not a validator vote.',
  },
  {
    meta: [
      { Icon: Layers, text: 'AMM · Lending · Vaults' },
      { Icon: Coins, text: 'Money market' },
      { Icon: Bitcoin, text: 'On Bitcoin L1' },
    ],
    title: ['DeFi,', 'settled on Bitcoin.'],
    desc: 'Token, a constant-product AMM, lending, an interest-bearing money market, and yield vaults — composable programs that settle on Bitcoin.',
  },
  {
    meta: [
      { Icon: KeyRound, text: 'BIP-340 Schnorr' },
      { Icon: ShieldCheck, text: 'Replay-protected' },
      { Icon: Cpu, text: 'Atomic state' },
    ],
    title: ['Real signatures.', 'No replays.'],
    desc: 'Every transaction is Schnorr-verified, replay-protected by a recent blockhash and chain id, and committed atomically.',
  },
  {
    meta: [
      { Icon: Github, text: 'MIT licensed' },
      { Icon: Cpu, text: 'Written in Rust' },
      { Icon: ShieldCheck, text: '182 tests' },
    ],
    title: ['Open source.', 'Come build.'],
    desc: 'Fully open source and contributable — pick a roadmap item and open a pull request.',
  },
]

const VIDEO =
  'https://d8j0ntlcm91z4.cloudfront.net/user_38xzZboKViGWJOttwIXH07lWA1P/hf_20260406_094145_4a271a6c-3869-4f1c-8aa7-aeb0cb227994.mp4'

export default function App() {
  const [open, setOpen] = useState(false)
  const [slide, setSlide] = useState(0)

  const prev = () => setSlide((s) => (s - 1 + SLIDES.length) % SLIDES.length)
  const next = () => setSlide((s) => (s + 1) % SLIDES.length)
  const current = SLIDES[slide]

  return (
    <div className="relative min-h-screen w-full overflow-hidden bg-black text-white">
      {/* ── Background video (z-0) ──────────────────────────────── */}
      <video
        className="fixed inset-0 z-0 h-full w-full object-cover"
        src={VIDEO}
        autoPlay
        loop
        muted
        playsInline
      />

      {/* ── Bottom blur overlay, no darkening (z-1) ─────────────── */}
      <div
        className="pointer-events-none fixed inset-0 z-[1] backdrop-blur-xl"
        style={{
          WebkitMaskImage: 'linear-gradient(to top, black 0%, transparent 45%)',
          maskImage: 'linear-gradient(to top, black 0%, transparent 45%)',
        }}
      />

      {/* ── Foreground column ───────────────────────────────────── */}
      <div className="relative z-10 flex min-h-screen flex-col">
        {/* Navbar */}
        <nav className="relative z-50 flex items-center justify-between px-4 py-4 sm:px-6 md:px-12 md:py-6">
          {/* Logo */}
          <a
            href={GITHUB}
            className="animate-blur-fade-up flex h-8 items-center text-xl font-bold tracking-tight md:h-10 md:text-2xl"
            style={{ animationDelay: '0ms' }}
          >
            HIMSHA
          </a>

          {/* Center links (lg+) */}
          <ul className="hidden items-center gap-8 lg:flex">
            {NAV.map((item, i) => (
              <li key={item.label}>
                <a
                  href={item.href}
                  className="animate-blur-fade-up text-sm text-white/90 transition-colors hover:text-gray-300"
                  style={{ animationDelay: `${100 + i * 50}ms` }}
                >
                  {item.label}
                </a>
              </li>
            ))}
          </ul>

          {/* Right actions */}
          <div className="flex items-center gap-3">
            <a
              href={`${GITHUB}/tree/main/docs`}
              className="liquid-glass animate-blur-fade-up hidden items-center gap-2 rounded-full px-4 py-2 text-sm text-white sm:flex md:px-6"
              style={{ animationDelay: '350ms' }}
            >
              Search
              <Search size={18} />
            </a>
            <a
              href={GITHUB}
              aria-label="GitHub repository"
              className="liquid-glass animate-blur-fade-up hidden h-10 w-10 items-center justify-center rounded-full sm:flex"
              style={{ animationDelay: '400ms' }}
            >
              <Github size={18} />
            </a>

            {/* Hamburger (below lg) */}
            <button
              onClick={() => setOpen((v) => !v)}
              aria-label="Toggle menu"
              className="liquid-glass animate-blur-fade-up relative flex h-10 w-10 items-center justify-center rounded-full lg:hidden"
              style={{ animationDelay: '350ms' }}
            >
              <span
                className={`absolute transition-all duration-500 ease-out ${
                  open ? 'scale-50 rotate-180 opacity-0' : 'scale-100 rotate-0 opacity-100'
                }`}
              >
                <Menu size={18} />
              </span>
              <span
                className={`absolute transition-all duration-500 ease-out ${
                  open ? 'scale-100 rotate-0 opacity-100' : 'scale-50 rotate-180 opacity-0'
                }`}
              >
                <X size={18} />
              </span>
            </button>
          </div>
        </nav>

        {/* Mobile menu */}
        <div
          className={`absolute left-0 right-0 top-[72px] z-40 mx-4 rounded-2xl border-b border-t border-gray-800 bg-gray-900/95 shadow-2xl backdrop-blur-lg transition-all duration-500 ease-out lg:hidden ${
            open
              ? 'translate-y-0 opacity-100'
              : 'pointer-events-none -translate-y-4 opacity-0'
          }`}
        >
          <div className="flex flex-col p-3">
            {NAV.map((item, i) => (
              <a
                key={item.label}
                href={item.href}
                className="rounded-lg px-3 py-3 text-white/90 transition-all hover:bg-gray-800/50"
                style={{
                  transitionDelay: open ? `${i * 50}ms` : '0ms',
                  transform: open ? 'translateX(0)' : 'translateX(-12px)',
                  opacity: open ? 1 : 0,
                }}
              >
                {item.label}
              </a>
            ))}
            <div className="mt-2 flex gap-3 border-t border-gray-800 pt-3 sm:hidden">
              <a
                href={`${GITHUB}/tree/main/docs`}
                className="liquid-glass flex flex-1 items-center justify-center gap-2 rounded-full px-4 py-2 text-sm"
              >
                Search <Search size={16} />
              </a>
              <a
                href={GITHUB}
                aria-label="GitHub"
                className="liquid-glass flex h-10 w-10 items-center justify-center rounded-full"
              >
                <Github size={18} />
              </a>
            </div>
          </div>
        </div>

        {/* Hero content (bottom) */}
        <div className="z-10 flex flex-1 flex-col justify-end px-4 pb-8 sm:px-6 md:px-12 md:pb-16">
          <div className="flex flex-col items-end gap-8 md:flex-row">
            {/* Left — keyed by slide so it re-animates on Prev/Next */}
            <div className="flex-1">
              <div key={slide}>
                {/* Metadata */}
                <div
                  className="animate-blur-fade-up mb-6 flex flex-wrap items-center gap-3 text-xs sm:gap-6 sm:text-sm md:mb-8"
                  style={{ animationDelay: '60ms' }}
                >
                  {current.meta.map(({ Icon, text }, i) => (
                    <span key={i} className="flex items-center gap-2">
                      <Icon size={16} className="sm:h-5 sm:w-5" />
                      <span className="font-medium">{text}</span>
                    </span>
                  ))}
                </div>

                {/* Title */}
                <h1
                  className="animate-blur-fade-up mb-4 text-3xl font-normal sm:text-5xl md:mb-6 md:text-6xl lg:text-7xl"
                  style={{ animationDelay: '120ms', letterSpacing: '-0.04em' }}
                >
                  {current.title[0]}
                  <br />
                  {current.title[1]}
                </h1>

                {/* Description */}
                <p
                  className="animate-blur-fade-up mb-6 max-w-2xl text-base text-gray-300 sm:text-lg md:mb-12 md:text-xl"
                  style={{ animationDelay: '200ms' }}
                >
                  {current.desc}
                </p>
              </div>

              {/* CTAs (static) */}
              <div className="flex flex-wrap gap-3 sm:gap-4">
                <a
                  href={`${GITHUB}#quick-start`}
                  className="animate-blur-fade-up flex items-center gap-2 rounded-full bg-white px-6 py-2.5 font-medium text-black transition-colors hover:bg-gray-200 sm:px-8 sm:py-3"
                  style={{ animationDelay: '600ms' }}
                >
                  Get Started
                  <ArrowRight size={18} />
                </a>
                <a
                  href={GITHUB}
                  className="liquid-glass animate-blur-fade-up flex items-center gap-2 rounded-full px-6 py-2.5 font-medium sm:px-8 sm:py-3"
                  style={{ animationDelay: '700ms' }}
                >
                  <Github size={18} />
                  View on GitHub
                </a>
              </div>
            </div>

            {/* Right: working carousel arrows + dots */}
            <div className="flex flex-col items-start gap-3 md:items-end">
              <div className="flex gap-3">
                <button
                  onClick={prev}
                  aria-label="Previous"
                  className="liquid-glass animate-blur-fade-up flex items-center gap-2 rounded-full px-4 py-2.5 transition-opacity hover:opacity-80 sm:px-6 sm:py-3"
                  style={{ animationDelay: '800ms' }}
                >
                  <ChevronLeft size={18} />
                  <span className="hidden sm:inline">Previous</span>
                </button>
                <button
                  onClick={next}
                  aria-label="Next"
                  className="liquid-glass animate-blur-fade-up flex items-center gap-2 rounded-full px-4 py-2.5 transition-opacity hover:opacity-80 sm:px-6 sm:py-3"
                  style={{ animationDelay: '900ms' }}
                >
                  <span className="hidden sm:inline">Next</span>
                  <ChevronRight size={18} />
                </button>
              </div>
              {/* Slide indicator dots */}
              <div className="flex items-center gap-2">
                {SLIDES.map((_, i) => (
                  <button
                    key={i}
                    onClick={() => setSlide(i)}
                    aria-label={`Go to slide ${i + 1}`}
                    className={`h-1.5 rounded-full transition-all duration-300 ${
                      i === slide ? 'w-6 bg-white' : 'w-1.5 bg-white/40 hover:bg-white/60'
                    }`}
                  />
                ))}
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
