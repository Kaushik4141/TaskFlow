/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{ts,tsx}'],
  theme: {
    extend: {
      fontFamily: {
        sans: ['Inter', 'Aptos', 'Segoe UI Variable', 'Segoe UI', 'system-ui', 'sans-serif'],
        display: ['Studio Feixen Sans', 'Inter', 'sans-serif'],
        mono: ['JetBrains Mono', 'Cascadia Code', 'Consolas', 'monospace'],
      },
      colors: {
        noir: {
          950: '#09090b',
          900: '#121214',
          850: '#1a1a1d',
          800: '#222225',
          750: '#2d2d30',
          700: '#38383b',
        },
        // Brand color ramp (uses CSS vars from index.css for themes)
        brand: {
          600: 'rgb(var(--brand-600) / <alpha-value>)',
          500: 'rgb(var(--brand-500) / <alpha-value>)',
          400: 'rgb(var(--brand-400) / <alpha-value>)',
          300: 'rgb(var(--brand-300) / <alpha-value>)',
          200: 'rgb(var(--brand-200) / <alpha-value>)',
          100: 'rgb(var(--brand-100) / <alpha-value>)',
        },
      },
      textColor: {
        // Warm off-white ink scale (opacity-driven off #F4ECF0 for controlled contrast)
        ink: '#F4ECF0',
      },
      borderRadius: {
        // Tighten the scale — no over-rounding
        xl: '12px',
        '2xl': '16px',
      },
      boxShadow: {
        // Subtle theme glows for primary surfaces / active states
        glow: '0 0 0 1px rgb(var(--brand-300) / 0.18), 0 8px 32px -8px rgb(var(--brand-300) / 0.35)',
        'glow-sm': '0 0 0 1px rgb(var(--brand-300) / 0.14), 0 4px 16px -6px rgb(var(--brand-300) / 0.28)',
        'glow-pink': '0 0 40px -4px rgb(var(--brand-300) / 0.25)',
        // Neutral elevation
        elevated: '0 8px 24px -12px rgba(0, 0, 0, 0.6)',
      },
      keyframes: {
        'pulse-ring': {
          '0%': { transform: 'scale(0.9)', opacity: '0.7' },
          '70%': { transform: 'scale(2.4)', opacity: '0' },
          '100%': { transform: 'scale(2.4)', opacity: '0' },
        },
        shimmer: {
          '0%': { backgroundPosition: '200% 0' },
          '100%': { backgroundPosition: '-200% 0' },
        },
        'toast-in': {
          '0%': { opacity: '0', transform: 'translateY(12px) scale(0.96)' },
          '100%': { opacity: '1', transform: 'translateY(0) scale(1)' },
        },
        'gradient-drift': {
          '0%, 100%': { transform: 'translate(0, 0) scale(1)' },
          '33%': { transform: 'translate(3%, -2%) scale(1.05)' },
          '66%': { transform: 'translate(-2%, 3%) scale(0.97)' },
        },
      },
      animation: {
        'pulse-ring': 'pulse-ring 2s cubic-bezier(0.4, 0, 0.6, 1) infinite',
        shimmer: 'shimmer 2s ease-in-out infinite',
        'toast-in': 'toast-in 0.25s ease-out',
        'gradient-drift': 'gradient-drift 18s ease-in-out infinite',
      },
    },
  },
  plugins: [],
}
