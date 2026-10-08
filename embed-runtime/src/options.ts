// Option values reach a runtime as JSON numbers or strings (the manifest keeps
// the author's text); a flag is on for `true`, a nonzero number, or the words
// true, yes, on and 1.
export function flag(v: unknown): boolean {
  if (typeof v === 'boolean') return v;
  if (typeof v === 'number') return v !== 0;
  return typeof v === 'string' && /^(?:true|yes|on|1)$/i.test(v.trim());
}
