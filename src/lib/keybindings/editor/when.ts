// Strict validator for `when` clause expressions (the runtime evaluator in
// `$lib/commands/when.ts` is lenient and never reports errors). Used by the
// keybinding editor before saving a user-edited expression.

export type WhenErrorCode = 'unexpectedChar' | 'unterminatedString' | 'unexpectedToken' | 'unexpectedEnd' | 'unclosedParen';

export interface WhenError {
  code: WhenErrorCode;
  /** Character offset in the expression. */
  at: number;
  token?: string;
}

interface Tok {
  v: string;
  at: number;
}

const TOKEN = /\s*(&&|\|\||==|!=|!|\(|\)|'[^']*'|"[^"]*"|[\w.\-:/]+)/y;
const OPERATORS = new Set(['&&', '||', '==', '!=', ')']);

function tokenize(s: string): Tok[] | WhenError {
  const out: Tok[] = [];
  let pos = 0;
  while (pos < s.length) {
    if (/^\s*$/.test(s.slice(pos))) break;
    TOKEN.lastIndex = pos;
    const m = TOKEN.exec(s);
    if (!m) {
      let at = pos;
      while (at < s.length && /\s/.test(s[at])) at++;
      const ch = s[at];
      return { code: ch === "'" || ch === '"' ? 'unterminatedString' : 'unexpectedChar', at, token: ch };
    }
    out.push({ v: m[1], at: m.index + m[0].length - m[1].length });
    pos = TOKEN.lastIndex;
  }
  return out;
}

/** Returns null when the expression is valid (empty is valid: "always"). */
export function validateWhen(expr: string): WhenError | null {
  const toks = tokenize(expr);
  if (!Array.isArray(toks)) return toks;
  if (!toks.length) return null;
  let i = 0;
  const end = expr.length;

  const fail = (code: WhenErrorCode, tok?: Tok): never => {
    throw { code, at: tok?.at ?? end, token: tok?.v } satisfies WhenError;
  };

  const operand = () => {
    const tok = toks[i];
    if (!tok) fail('unexpectedEnd');
    if (OPERATORS.has(tok.v) || tok.v === '!' || tok.v === '(') fail('unexpectedToken', tok);
    i++;
  };
  const primary = (): void => {
    const tok = toks[i];
    if (!tok) return fail('unexpectedEnd');
    if (tok.v === '!') {
      i++;
      return primary();
    }
    if (tok.v === '(') {
      i++;
      or();
      if (toks[i]?.v !== ')') fail('unclosedParen', toks[i] ?? tok);
      i++;
      return;
    }
    operand();
  };
  const cmp = () => {
    primary();
    if (toks[i]?.v === '==' || toks[i]?.v === '!=') {
      i++;
      operand();
    }
  };
  const and = () => {
    cmp();
    while (toks[i]?.v === '&&') {
      i++;
      cmp();
    }
  };
  const or = () => {
    and();
    while (toks[i]?.v === '||') {
      i++;
      and();
    }
  };

  try {
    or();
    if (i < toks.length) fail('unexpectedToken', toks[i]);
    return null;
  } catch (e) {
    return e as WhenError;
  }
}

/** Context keys referenced by an expression (for hints). */
export function whenKeys(expr: string): string[] {
  const toks = tokenize(expr);
  if (!Array.isArray(toks)) return [];
  return toks.map((t) => t.v).filter((v) => /^[A-Za-z_][\w.\-:/]*$/.test(v) && v !== 'true' && v !== 'false');
}
