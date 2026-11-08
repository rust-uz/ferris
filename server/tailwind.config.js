/** Tailwind config used to build static/tailwind.css (replaces the runtime CDN). */
module.exports = {
  darkMode: 'class',
  content: [
    './templates/**/*.html',
    '../static/js/mascot-widget.js',
    '../static/app.js',
  ],
  theme: {
    extend: {
      fontFamily: {
        sans: ['"Plus Jakarta Sans"', 'sans-serif'],
        display: ['"Familjen Grotesk"', '"Plus Jakarta Sans"', 'sans-serif'],
        mono: ['"JetBrains Mono"', 'monospace'],
      },
      colors: {
        paper: 'rgb(var(--c-paper) / <alpha-value>)',
        sunk: 'rgb(var(--c-sunk) / <alpha-value>)',
        surface: 'rgb(var(--c-surface) / <alpha-value>)',
        surface2: 'rgb(var(--c-surface2) / <alpha-value>)',
        surface3: 'rgb(var(--c-surface3) / <alpha-value>)',
        ink: 'rgb(var(--c-ink) / <alpha-value>)',
        ink2: 'rgb(var(--c-ink2) / <alpha-value>)',
        muted: 'rgb(var(--c-muted) / <alpha-value>)',
        faint: 'rgb(var(--c-faint) / <alpha-value>)',
        dim: 'rgb(var(--c-dim) / <alpha-value>)',
        'on-accent': 'rgb(var(--c-on-accent) / <alpha-value>)',
        brand: {
          orange: '#f74c00',
          accent: '#ff6a00',
          dark: '#09090b',
        },
      },
    },
  },
  plugins: [],
};
