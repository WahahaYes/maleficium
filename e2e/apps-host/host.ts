// A minimal MCP Apps host for the headless test: the page plays the chat
// client. It renders a View's html in a sandboxed iframe, speaks the Apps
// bridge to it, and forwards the View's tool calls to window.mcpCall, which
// run.mjs backs with the real maleficium-mcp over stdio.
import { AppBridge, PostMessageTransport } from '@modelcontextprotocol/ext-apps/app-bridge';

interface RunOptions {
  html: string;
  toolInput: { arguments: Record<string, unknown> };
  toolResult: { content: unknown[]; structuredContent?: unknown };
  theme: 'light' | 'dark';
}

declare global {
  interface Window {
    mcpCall: (params: unknown) => Promise<unknown>;
    runView: (o: RunOptions) => Promise<void>;
    viewConnected: Promise<void>;
  }
}

window.runView = async (o) => {
  document.documentElement.style.colorScheme = o.theme;
  document.body.style.background = o.theme === 'dark' ? '#0d1117' : '#fff';
  const frame = document.getElementById('view') as HTMLIFrameElement;
  const bridge = new AppBridge(
    null,
    { name: 'test-host', version: '0' },
    { serverTools: {}, updateModelContext: { text: {} } },
    {
      hostContext: {
        theme: o.theme,
        styles: {
          variables: {
            '--color-background-primary': o.theme === 'dark' ? '#0d1117' : '#ffffff',
            '--color-background-secondary': o.theme === 'dark' ? '#161b22' : '#f6f8fa',
            '--color-text-primary': o.theme === 'dark' ? '#e6edf3' : '#1f2328',
            '--color-text-secondary': o.theme === 'dark' ? '#9198a1' : '#59636e',
            '--color-border-primary': o.theme === 'dark' ? '#30363d' : '#d1d9e0',
          },
        },
      },
    },
  );
  bridge.oncalltool = async (params) => (await window.mcpCall(params)) as never;
  const ready = new Promise<void>((resolve) => {
    bridge.oninitialized = () => {
      void bridge.sendToolInput(o.toolInput);
      void bridge.sendToolResult(o.toolResult as never);
      resolve();
    };
  });
  frame.srcdoc = o.html;
  await bridge.connect(new PostMessageTransport(frame.contentWindow!, frame.contentWindow!));
  await ready;
};
