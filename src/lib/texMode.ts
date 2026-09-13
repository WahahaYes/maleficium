// texMode.ts — lightweight LaTeX grammar for CodeMirror (`StreamLanguage`).
//
// Growth cap: tokenize-only, no parse tree retained; viewport render unchanged.
// Own grammar (ideas-only): commands, comments, math, braces.

import { StreamLanguage } from '@codemirror/language';

export const texMode = StreamLanguage.define({
  token(stream) {
    if (stream.eat('%')) {
      stream.skipToEnd();
      return 'comment';
    }
    if (stream.match(/\\[a-zA-Z@]+|\\[^a-zA-Z@]/)) return 'keyword';
    if (stream.eat('$')) {
      if (stream.eat('$')) stream.skipTo('$$');
      else stream.skipTo('$');
      stream.eat('$');
      return 'string';
    }
    if (stream.match(/\\\[|\\\]|\\\(|\\\)/)) return 'string';
    if (stream.eat('{') || stream.eat('}')) return 'bracket';
    stream.next();
    return null;
  },
});
