// The live-equation formula language: an arithmetic subset that TeX's l3fp
// also evaluates, so the PDF typesets the same values the reader computes.
// Formulas are data, never code: no eval, so no policy needs unsafe-eval.
// Hostile input is bounded by MAX_LENGTH and MAX_DEPTH and never throws.
// The shared vectors (expr.vectors.json) hold this interpreter and the .sty's
// l3fp path to one answer.
//
// Loosest to tightest:
//   expr  := or ('?' expr ':' expr)?             right-assoc
//   or    := and ('||' and)*                     value-returning, as l3fp
//   and   := cmp ('&&' cmp)*
//   cmp   := add (('<'|'>'|'<='|'>='|'=='|'!=') add)?   no chains
//   add   := mul (('+'|'-') mul)*
//   mul   := unary (('*'|'/') unary)*
//   unary := ('-'|'+'|'!') unary | pow           -2^2 = -4, !1<2 = (!1)<2
//   pow   := atom ('^' unary)?                   right-assoc, 2^-1 allowed
//   atom  := number | name | fn '(' expr (',' expr)* ')' | '(' expr ')'
// l3fp also takes implicit products (2pi), spaces inside numbers (1 0 is
// 10), **, a single =, chained comparisons and more functions; all of those
// are rejected here, one spelling per thing.
//
// Numbers: l3fp is 16-digit decimal, JS binary double. Each rounding first
// takes the value to 16 significant digits, then to the places asked, both
// ties to even, as l3fp's round does. Results past about 1e308 overflow
// here and not in l3fp.

export type Expr =
  | { k: 'num'; v: number }
  | { k: 'name'; name: string }
  | { k: 'unary'; op: '-' | '+' | '!'; a: Expr }
  | { k: 'binary'; op: string; a: Expr; b: Expr }
  | { k: 'cond'; c: Expr; a: Expr; b: Expr }
  | { k: 'call'; fn: string; args: Expr[] };

export type Parsed = { ok: true; expr: Expr } | { ok: false; at: number; message: string };

export const MAX_LENGTH = 1000;
export const MAX_DEPTH = 64;

/** Each function's arity: [min, max] arguments. */
export const FUNCTIONS: ReadonlyMap<string, readonly [number, number]> = new Map<
  string,
  readonly [number, number]
>([
  ['abs', [1, 1]],
  ['exp', [1, 1]],
  ['ln', [1, 1]],
  ['sqrt', [1, 1]],
  ['sin', [1, 1]],
  ['cos', [1, 1]],
  ['tan', [1, 1]],
  ['asin', [1, 1]],
  ['acos', [1, 1]],
  ['atan', [1, 2]],
  ['floor', [1, 1]],
  ['ceil', [1, 1]],
  ['trunc', [1, 1]],
  ['sign', [1, 1]],
  ['round', [1, 2]],
  ['min', [1, Infinity]],
  ['max', [1, Infinity]],
]);

/** Names a param may not take. */
export function reservedName(name: string): boolean {
  return name === 'pi' || FUNCTIONS.has(name);
}

/** Whether `name` is a valid param name: letters, then letters or digits, not reserved. */
export function validParamName(name: string): boolean {
  return /^[A-Za-z][A-Za-z0-9]*$/.test(name) && !reservedName(name);
}

type Tok =
  | { t: 'num'; v: number; at: number }
  | { t: 'name'; v: string; at: number }
  | { t: 'op'; v: string; at: number };

const OPS = [
  '<=',
  '>=',
  '==',
  '!=',
  '&&',
  '||',
  '+',
  '-',
  '*',
  '/',
  '^',
  '<',
  '>',
  '!',
  '?',
  ':',
  '(',
  ')',
  ',',
];

class Fail {
  constructor(
    readonly at: number,
    readonly message: string,
  ) {}
}

function tokenize(src: string): Tok[] {
  const out: Tok[] = [];
  let i = 0;
  while (i < src.length) {
    const c = src[i];
    if (c === ' ' || c === '\t' || c === '\n' || c === '\r') {
      i++;
      continue;
    }
    const num = /^(?:[0-9]+(?:\.[0-9]*)?|\.[0-9]+)(?:[eE][+-]?[0-9]+)?/.exec(src.slice(i));
    if (num) {
      out.push({ t: 'num', v: Number(num[0]), at: i });
      i += num[0].length;
      continue;
    }
    const name = /^[A-Za-z][A-Za-z0-9]*/.exec(src.slice(i));
    if (name) {
      out.push({ t: 'name', v: name[0], at: i });
      i += name[0].length;
      continue;
    }
    if (src.startsWith('**', i)) throw new Fail(i, 'use ^ for powers, not **');
    const op = OPS.find((o) => src.startsWith(o, i));
    if (!op) {
      if (c === '=') throw new Fail(i, 'use == to compare, not =');
      throw new Fail(i, `unexpected character '${c}'`);
    }
    out.push({ t: 'op', v: op, at: i });
    i += op.length;
  }
  // Two operands side by side is implicit multiplication, which l3fp reads
  // as a product and spaces inside a number, which l3fp joins: both rejected.
  for (let j = 1; j < out.length; j++) {
    const a = out[j - 1];
    const b = out[j];
    const ends = a.t !== 'op' || a.v === ')';
    const starts = b.t !== 'op' || b.v === '(';
    const call = a.t === 'name' && FUNCTIONS.has(a.v) && b.t === 'op' && b.v === '(';
    if (ends && starts && !call) {
      throw new Fail(b.at, 'write * between factors; implicit multiplication is not allowed');
    }
  }
  return out;
}

class Parser {
  private i = 0;
  private depth = 0;
  constructor(
    private readonly toks: Tok[],
    private readonly names: ReadonlySet<string>,
    private readonly end: number,
  ) {}

  parse(): Expr {
    const e = this.expr();
    if (this.i < this.toks.length) throw new Fail(this.toks[this.i].at, 'unexpected input');
    return e;
  }

  private peek(v: string): boolean {
    const t = this.toks[this.i];
    return t !== undefined && t.t === 'op' && t.v === v;
  }

  private take(v: string): boolean {
    if (!this.peek(v)) return false;
    this.i++;
    return true;
  }

  private at(): number {
    return this.toks[this.i]?.at ?? this.end;
  }

  private nest<T>(f: () => T): T {
    if (++this.depth > MAX_DEPTH) throw new Fail(this.at(), 'formula is nested too deeply');
    try {
      return f();
    } finally {
      this.depth--;
    }
  }

  private expr(): Expr {
    return this.nest(() => {
      const c = this.or();
      if (!this.take('?')) return c;
      const a = this.expr();
      if (!this.take(':')) throw new Fail(this.at(), "expected ':'");
      return { k: 'cond', c, a, b: this.expr() };
    });
  }

  private left(next: () => Expr, ops: string[]): Expr {
    let a = next();
    for (;;) {
      const op = ops.find((o) => this.peek(o));
      if (!op) return a;
      this.i++;
      a = { k: 'binary', op, a, b: next() };
    }
  }

  private or(): Expr {
    return this.left(() => this.and(), ['||']);
  }

  private and(): Expr {
    return this.left(() => this.cmp(), ['&&']);
  }

  private cmp(): Expr {
    const cmps = ['<=', '>=', '==', '!=', '<', '>'];
    const a = this.add();
    const op = cmps.find((o) => this.peek(o));
    if (!op) return a;
    this.i++;
    const b = this.add();
    if (cmps.some((o) => this.peek(o))) {
      throw new Fail(this.at(), 'chained comparisons are not allowed; join them with &&');
    }
    return { k: 'binary', op, a, b };
  }

  private add(): Expr {
    return this.left(() => this.mul(), ['+', '-']);
  }

  private mul(): Expr {
    return this.left(() => this.unary(), ['*', '/']);
  }

  private unary(): Expr {
    return this.nest(() => {
      for (const op of ['-', '+', '!'] as const) {
        if (this.take(op)) return { k: 'unary', op, a: this.unary() };
      }
      const a = this.atom();
      return this.take('^') ? { k: 'binary', op: '^', a, b: this.unary() } : a;
    });
  }

  private atom(): Expr {
    const t = this.toks[this.i];
    if (!t) throw new Fail(this.end, 'formula ends too soon');
    if (t.t === 'num') {
      this.i++;
      return { k: 'num', v: t.v };
    }
    if (t.t === 'name') {
      this.i++;
      if (FUNCTIONS.has(t.v)) return this.call(t.v, t.at);
      if (t.v === 'pi' || this.names.has(t.v)) return { k: 'name', name: t.v };
      throw new Fail(t.at, `unknown name '${t.v}'`);
    }
    if (this.take('(')) {
      const e = this.expr();
      if (!this.take(')')) throw new Fail(this.at(), "expected ')'");
      return e;
    }
    throw new Fail(t.at, `unexpected '${t.v}'`);
  }

  private call(fn: string, at: number): Expr {
    if (!this.take('(')) throw new Fail(this.at(), `expected '(' after ${fn}`);
    const args = [this.expr()];
    while (this.take(',')) args.push(this.expr());
    if (!this.take(')')) throw new Fail(this.at(), "expected ')'");
    const [lo, hi] = FUNCTIONS.get(fn) ?? [1, 1];
    if (args.length < lo || args.length > hi) {
      const want = lo === hi ? `${lo}` : hi === Infinity ? `at least ${lo}` : `${lo} or ${hi}`;
      throw new Fail(at, `${fn} takes ${want} argument${lo === 1 && hi === 1 ? '' : 's'}`);
    }
    return { k: 'call', fn, args };
  }
}

/** Parses a formula whose only names are `names` (the equation's params) and `pi`. */
export function parse(src: string, names: readonly string[]): Parsed {
  if (src.length > MAX_LENGTH) return { ok: false, at: MAX_LENGTH, message: 'formula is too long' };
  try {
    const expr = new Parser(tokenize(src), new Set(names), src.length).parse();
    return { ok: true, expr };
  } catch (e) {
    if (e instanceof Fail) return { ok: false, at: e.at, message: e.message };
    throw e;
  }
}

const truthy = (v: number) => v !== 0;

/** Evaluates a parsed formula. NaN or an infinity means undefined. */
export function evaluate(e: Expr, vars: Readonly<Record<string, number>>): number {
  switch (e.k) {
    case 'num':
      return e.v;
    case 'name':
      return e.name === 'pi' ? Math.PI : (vars[e.name] ?? NaN);
    case 'unary': {
      const a = evaluate(e.a, vars);
      return e.op === '-' ? -a : e.op === '+' ? a : truthy(a) ? 0 : 1;
    }
    case 'cond':
      return truthy(evaluate(e.c, vars)) ? evaluate(e.a, vars) : evaluate(e.b, vars);
    case 'binary': {
      const a = evaluate(e.a, vars);
      if (e.op === '&&') return truthy(a) ? evaluate(e.b, vars) : a;
      if (e.op === '||') return truthy(a) ? a : evaluate(e.b, vars);
      const b = evaluate(e.b, vars);
      switch (e.op) {
        case '+':
          return a + b;
        case '-':
          return a - b;
        case '*':
          return a * b;
        case '/':
          return a / b;
        case '^':
          return Math.pow(a, b);
        case '<':
          return +(a < b);
        case '>':
          return +(a > b);
        case '<=':
          return +(a <= b);
        case '>=':
          return +(a >= b);
        case '==':
          return +(a === b);
        default:
          return +(a !== b);
      }
    }
    case 'call': {
      const x = e.args.map((a) => evaluate(a, vars));
      switch (e.fn) {
        case 'atan':
          return x.length === 2 ? Math.atan2(x[0], x[1]) : Math.atan(x[0]);
        case 'round':
          return roundDecimal(x[0], x.length === 2 ? Math.trunc(x[1]) : 0);
        case 'min':
          return Math.min(...x);
        case 'max':
          return Math.max(...x);
        case 'ln':
          return Math.log(x[0]);
        default:
          return (Math[e.fn as 'abs'] as (v: number) => number)(x[0]);
      }
    }
  }
}

// -- decimal rounding, as l3fp does it ------------------------------------

/** A finite number as decimal digits and the power of ten of its last digit. */
function decimal(v: number): { neg: boolean; digits: string; exp: number } {
  const [mant, e] = Math.abs(v).toExponential().split('e');
  const [int, frac = ''] = mant.split('.');
  const digits = (int + frac).replace(/^0+(?=\d)/, '');
  return { neg: v < 0 || Object.is(v, -0), digits, exp: Number(e) - frac.length };
}

/** Rounds a digit string to drop its last `drop` digits, ties to even. */
function roundDigits(digits: string, drop: number): string {
  if (drop <= 0) return digits;
  if (drop > digits.length) return '0';
  const keep = digits.slice(0, digits.length - drop) || '0';
  const rest = digits.slice(digits.length - drop);
  const half = '5' + '0'.repeat(drop - 1);
  const up = rest > half || (rest === half && Number(keep[keep.length - 1]) % 2 === 1);
  if (!up) return keep;
  const out = keep.split('');
  let i = out.length - 1;
  while (i >= 0 && out[i] === '9') out[i--] = '0';
  if (i < 0) out.unshift('1');
  else out[i] = String(Number(out[i]) + 1);
  return out.join('');
}

/** l3fp's precision: the value rounded to 16 significant digits, ties to even. */
function sixteen(v: number): { neg: boolean; digits: string; exp: number } {
  const d = decimal(v);
  const drop = d.digits.length - 16;
  if (drop <= 0) return d;
  return { neg: d.neg, digits: roundDigits(d.digits, drop), exp: d.exp + drop };
}

/** Rounds `v` to `places` decimals (negative: tens, hundreds), ties to even. */
export function roundDecimal(v: number, places: number): number {
  if (!Number.isFinite(v)) return v;
  const d = sixteen(v);
  const drop = -places - d.exp;
  if (drop <= 0) return v;
  const r = roundDigits(d.digits, drop);
  return Number(`${d.neg ? '-' : ''}${r}e${d.exp + drop}`);
}

/** Shows `v` with exactly `places` decimals (0 or more), or null when it is undefined. */
export function format(v: number, places: number): string | null {
  if (!Number.isFinite(v)) return null;
  const d = sixteen(v);
  const drop = -places - d.exp;
  let digits = drop > 0 ? roundDigits(d.digits, drop) : d.digits;
  let exp = drop > 0 ? d.exp + drop : d.exp;
  // Pad so the digit string reaches the last decimal place shown.
  digits += '0'.repeat(Math.max(0, exp + places));
  exp = -places;
  digits = digits.padStart(places + 1, '0');
  const int = digits.slice(0, digits.length - places).replace(/^0+(?=\d)/, '');
  const frac = digits.slice(digits.length - places);
  const zero = /^0*$/.test(int + frac);
  const body = places > 0 ? `${int}.${frac}` : int;
  return d.neg && !zero ? `-${body}` : body;
}
