// CLI fixture generator for the e2e harnesses. Writes generated documents into
// a caller-supplied directory (harnesses pass an OS tmp path) and prints the
// main file it wrote.
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import process from 'node:process';
import { LOREM, makeSingle } from './fixtures';

/**
 * Writes `files` sibling .tex fragments at depth 1 plus `deep/` and `small/`
 * subdirectories, and a main that inputs every depth-1 fragment.
 */
export function makeFlatTree(root: string, files: number): string {
  const deep = join(root, 'deep');
  const small = join(root, 'small');
  mkdirSync(deep, { recursive: true });
  mkdirSync(small, { recursive: true });
  const inputs: string[] = [];
  for (let i = 1; i <= files; i++) {
    writeFileSync(join(root, `f${i}.tex`), `\\section{Part ${i}}\n${LOREM}\n`);
    inputs.push(`\\input{f${i}}`);
    writeFileSync(join(deep, `d${i}.tex`), `${LOREM}\n`);
  }
  for (let i = 1; i <= 10; i++) {
    writeFileSync(join(small, `s${i}.tex`), `${LOREM}\n`);
  }
  const mainPath = join(root, 'main.tex');
  writeFileSync(
    mainPath,
    `\\documentclass{article}\n\\begin{document}\n${inputs.join('\n')}\n\\end{document}\n`,
  );
  return mainPath;
}

function usage(): never {
  process.stderr.write('usage: make-fixture <pages|flat> <count> <dir>\n');
  process.exit(2);
}

const [kind, countArg, dir] = process.argv.slice(2);
const count = Number(countArg);
if (!kind || !Number.isFinite(count) || count <= 0 || !dir) usage();
mkdirSync(dir, { recursive: true });
if (kind === 'pages') {
  process.stdout.write(makeSingle(dir, count) + '\n');
} else if (kind === 'flat') {
  process.stdout.write(makeFlatTree(dir, count) + '\n');
} else {
  usage();
}
