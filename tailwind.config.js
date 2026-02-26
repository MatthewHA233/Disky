/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        background: '#FAF8F5',
        surface: '#FFFFFF',
        obsidian: '#0D0D12',
        champagne: '#C9A84C',
        slate: '#2A2A35',
        muted: '#E5E3DF',
        border: '#EAE6DF',
      },
      fontFamily: {
        sans: ['"Inter"', 'sans-serif'],
        serif: ['"Playfair Display"', 'serif'],
        mono: ['"JetBrains Mono"', 'monospace'],
      },
      animation: {
        'magnetic': 'magnetic 3s ease infinite',
      }
    },
  },
  plugins: [],
}
