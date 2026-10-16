// The plotly.js calls this runtime makes, on the core ./plotly.ts registers traces with.
declare module 'plotly.js/lib/core' {
  const Plotly: {
    register(modules: unknown[]): void;
    react(
      el: HTMLElement,
      data: unknown[],
      layout: Record<string, unknown>,
      config?: Record<string, unknown>,
    ): Promise<unknown>;
    toImage(
      el: HTMLElement,
      opts: { format: 'png'; width: number; height: number; scale: number },
    ): Promise<string>;
  };
  export default Plotly;
}
declare module 'plotly.js/lib/*' {
  const trace: unknown;
  export default trace;
}
