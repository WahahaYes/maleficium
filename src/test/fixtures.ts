import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { deflateSync } from 'node:zlib';

export const LOREM = 'Lorem ipsum dolor sit amet, consectetur adipiscing elit. ';

// Table-based CRC32
const CRC32_TABLE: number[] = [];
for (let i = 0; i < 256; i++) {
  let c = i;
  for (let j = 0; j < 8; j++) {
    c = (c & 1) ? (0xEDB88320 ^ (c >>> 1)) : (c >>> 1);
  }
  CRC32_TABLE[i] = c;
}

export function crc32(buf: Buffer): number {
  let cksum = 0xffffffff;
  for (let i = 0; i < buf.length; i++) {
    cksum = CRC32_TABLE[(cksum ^ buf[i]) & 0xff] ^ (cksum >>> 8);
  }
  return (cksum ^ 0xffffffff) >>> 0;
}

function makeChunk(type: string, data: Buffer): Buffer {
  const typeBuf = Buffer.from(type, 'ascii');
  const len = data.length;
  const chunk = Buffer.alloc(8 + len);
  chunk.writeUInt32BE(len, 0);
  typeBuf.copy(chunk, 4);
  data.copy(chunk, 8);
  const crc = crc32(Buffer.concat([typeBuf, data]));
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE(crc, 0);
  crcBuf.copy(chunk, 8 + len);
  return chunk;
}

export function makePng(w: number, h: number, r: number, g: number, b: number): Buffer {
  // PNG signature
  const signature = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  
  // IHDR chunk (13 bytes)
  const ihdrData = Buffer.alloc(13);
  ihdrData.writeUInt32BE(w, 0);
  ihdrData.writeUInt32BE(h, 4);
  ihdrData.writeUInt8(8, 8);   // bit depth
  ihdrData.writeUInt8(2, 9);   // color type (RGB)
  ihdrData.writeUInt8(0, 10);  // compression
  ihdrData.writeUInt8(0, 11);  // filter
  ihdrData.writeUInt8(0, 12);  // interlace
  const ihdrChunk = makeChunk('IHDR', ihdrData);
  
  // Filter-0 scanlines
  const scanlines: Buffer[] = [];
  for (let y = 0; y < h; y++) {
    const line = Buffer.alloc(1 + w * 3, b);
    for (let x = 0; x < w; x++) {
      line[1 + x * 3] = r;
      line[1 + x * 3 + 1] = g;
      line[1 + x * 3 + 2] = b;
    }
    scanlines.push(line);
  }
  const rawData = Buffer.concat(scanlines);
  const deflated = deflateSync(rawData);
  const idatChunk = makeChunk('IDAT', deflated);
  
  // IEND chunk
  const iendChunk = makeChunk('IEND', Buffer.alloc(0));
  
  return Buffer.concat([signature, ihdrChunk, idatChunk, iendChunk]);
}

export function makeTmpRoot(prefix = 'maleficium-test-'): string {
  return mkdtempSync(join(tmpdir(), prefix));
}

export function makeSingle(root: string, pages: number): string {
  const sections: string[] = [];
  for (let i = 0; i < pages; i++) {
    sections.push(`\\section{Section ${i + 1}}\n${LOREM.repeat(3)}\\newpage`);
  }
  const content = `\\documentclass{article}
\\begin{document}
${sections.join('\n')}
\\end{document}`;
  const docPath = join(root, 'doc.tex');
  writeFileSync(docPath, content);
  return docPath;
}

export function makeMulti(root: string, chapters: number): string {
  const inputLines: string[] = [];
  for (let i = 0; i < chapters; i++) {
    inputLines.push(`\\input{ch${i + 1}}`);
  }
  const mainContent = `\\documentclass{article}
\\begin{document}
${inputLines.join('\n')}
\\end{document}`;
  const mainPath = join(root, 'main.tex');
  writeFileSync(mainPath, mainContent);
  
  for (let i = 0; i < chapters; i++) {
    const chContent = `\\section{Chapter ${i + 1}}
${LOREM.repeat(3)}`;
    const chPath = join(root, `ch${i + 1}.tex`);
    writeFileSync(chPath, chContent);
  }
  
  return mainPath;
}

export function makeImageDoc(root: string, images: number): string {
  const includegraphics: string[] = [];
  for (let i = 0; i < images; i++) {
    includegraphics.push(`\\includegraphics{fig${i + 1}}`);
  }
  const content = `\\documentclass{article}
\\usepackage{graphicx}
\\begin{document}
${includegraphics.join('\n')}
\\end{document}`;
  const docPath = join(root, 'main.tex');
  writeFileSync(docPath, content);
  
  for (let i = 0; i < images; i++) {
    const r = (i * 50) % 256;
    const g = (i * 75) % 256;
    const b = (i * 100) % 256;
    const png = makePng(64, 64, r, g, b);
    const figPath = join(root, `fig${i + 1}.png`);
    writeFileSync(figPath, png);
  }
  
  return docPath;
}