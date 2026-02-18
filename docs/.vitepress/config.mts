import footnote from 'markdown-it-footnote';
import { defineConfig } from 'vitepress';

export default defineConfig({
  title: 'lattice',
  description: 'Local-first project board for human and agent execution',
  appearance: 'force-dark',
  cleanUrls: true,
  themeConfig: {
    logo: '/lattice-logo.svg',
    nav: [
      { text: 'Home', link: '/' },
      { text: 'Getting Started', link: '/getting-started' },
      { text: 'Usage', link: '/usage' },
      { text: 'MCP', link: '/mcp' },
    ],
    sidebar: [
      {
        text: 'Guide',
        items: [
          { text: 'Getting Started', link: '/getting-started' },
          { text: 'Usage', link: '/usage' },
          { text: 'MCP', link: '/mcp' },
        ],
      },
    ],
    search: {
      provider: 'local',
    },
    footer: {
      message: '<a href="https://github.com/gemologic/" target="_blank" rel="noreferrer">gemologic</a>',
    },
  },
  markdown: {
    config: (md) => {
      md.use(footnote);
    },
  },
});
