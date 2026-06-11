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
        // 组件中大量使用但此前未定义的颜色（Tailwind 对未知颜色类静默跳过，
        // 导致 bg-abyss 等完全不生效 —— 右键菜单透明就是这个原因）
        fiber: '#0D0D12',      // 最深背景（= --bg）
        abyss: '#13131A',      // 面板/气泡背景（= --surface）
        photon: '#C9A84C',     // 强调金（= --accent / champagne）
        textMain: '#FAF8F5',   // 主文字（= --text）
        'text-dim': '#888899', // 次要文字（= --text-dim）
        danger: '#E74C3C',
        success: '#4CAF50',
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
