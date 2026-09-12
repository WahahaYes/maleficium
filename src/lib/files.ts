export async function loadTexViaDialog(): Promise<{ name: string; content: string } | null> {
  // Stub: use document.createElement input type=file accept .tex, read text
  // TODO Tauri dialog+fs later
  return null;
}

export function saveTexToDisk(name: string, content: string): void {
  const blob = new Blob([content], { type: 'text/plain' });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement('a');
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  URL.revokeObjectURL(url);
}

export function helloName(): string {
  return 'Hello!';
}