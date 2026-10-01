/** The editor header's tooltip for the main-file chip: says where the pick came from. */
export function mainFileTip(mainFile: string | null, source: string): string {
  if (mainFile == null) return 'No main file detected';
  switch (source) {
    case 'config':
      return `Main file (your choice): ${mainFile}`;
    case 'magic':
      return `Main file (from %!TEX root): ${mainFile}`;
    case 'scan':
      return `Main file (auto-detected): ${mainFile}`;
    case 'single':
      return `Main file (only .tex file): ${mainFile}`;
    default:
      return `Main file: ${mainFile}`;
  }
}
