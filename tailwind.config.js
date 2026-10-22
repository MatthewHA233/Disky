/** @type {import('tailwindcss').Config} */
// 颜色名沿用历史命名（组件里大量使用），值全部指向 src/styles.css 里的 Fluent 令牌，
// 跟随系统浅色 / 深色。/10 这类透明度修饰由 Tailwind v4 的 color-mix 处理。
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
  ],
  theme: {
    extend: {
      colors: {
        fiber: 'var(--fl-bg)',         // 应用底色
        abyss: 'var(--fl-layer)',      // 面板 / 卡片
        card: 'var(--fl-card)',        // 浮层 / 对话框
        slate: 'var(--fl-stroke)',     // 描边 / 分隔 / 弱填充
        hover: 'var(--fl-hover)',      // 悬停填充
        press: 'var(--fl-press)',      // 按下填充
        photon: 'var(--fl-accent)',    // 强调色
        'on-accent': 'var(--fl-on-accent)',
        textMain: 'var(--fl-text)',    // 主文字
        'text-dim': 'var(--fl-text-2)',// 次要文字
        'text-faint': 'var(--fl-text-3)',
        border: 'var(--fl-text-2)',    // 历史上用作文件名颜色
        danger: 'var(--fl-danger)',
        critical: '#C42B1C',           // 删除类按钮底色：浅/深色下都配白字

        success: 'var(--fl-success)',
        caution: 'var(--fl-caution)',  // 提醒（已卸载的残留等）
        ai: 'var(--fl-ai)',
      },
      fontFamily: {
        sans: ['"Segoe UI Variable Text"', '"Segoe UI"', '-apple-system', 'BlinkMacSystemFont', '"PingFang SC"', '"Microsoft YaHei UI"', 'sans-serif'],
        // 界面里的数字统一用系统字体 + 等宽数字，不再用代码字体
        mono: ['"Segoe UI Variable Text"', '"Segoe UI"', '-apple-system', 'BlinkMacSystemFont', '"PingFang SC"', '"Microsoft YaHei UI"', 'sans-serif'],
        code: ['"Cascadia Mono"', 'Consolas', '"JetBrains Mono"', 'monospace'],
      },
    },
  },
  plugins: [],
}
