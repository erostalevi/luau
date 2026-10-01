// `when` clause expressions (VS Code style):
//   boardFocus && !inputFocus
//   boardType == 'files' || tabKind != 'board'
//   config.board.singleKeyShortcuts && (cardSelected || laneFocus)

type Ctx = (key: string) => unknown;
type Node =
  { t: 'lit'; v: unknown } | { t: 'key'; k: string } | { t: 'not'; e: Node } | { t: 'and' | 'or'; a: Node; b: Node } | { t: 'eq' | 'ne'; a: Node; b: Node };

function tokenize(s: string): string[] {
  const out: string[] = [];
  const re = /\s*(&&|\|\||==|!=|!|\(|\)|'[^']*'|"[^"]*"|[\w.\-:/]+)/y;
  let m: RegExpExecArray | null;
  re.lastIndex = 0;
  while (re.lastIndex < s.length && (m = re.exec(s))) out.push(m[1]);
  return out;
}

function parse(tokens: string[]): Node {
  let i = 0;
  const peek = () => tokens[i];
  const next = () => tokens[i++];
  const primary = (): Node => {
    const tok = next();
    if (tok === '!') return { t: 'not', e: primary() };
    if (tok === '(') {
      const e = or();
      next(); // ')'
      return e;
    }
    if (tok === undefined) return { t: 'lit', v: true };
    if (/^['"]/.test(tok)) return { t: 'lit', v: tok.slice(1, -1) };
    if (tok === 'true' || tok === 'false') return { t: 'lit', v: tok === 'true' };
    if (/^-?\d+(\.\d+)?$/.test(tok)) return { t: 'lit', v: Number(tok) };
    return { t: 'key', k: tok };
  };
  const cmp = (): Node => {
    const a = primary();
    if (peek() === '==' || peek() === '!=') {
      const op = next() === '==' ? 'eq' : 'ne';
      return { t: op, a, b: primary() };
    }
    return a;
  };
  const and = (): Node => {
    let a = cmp();
    while (peek() === '&&') {
      next();
      a = { t: 'and', a, b: cmp() };
    }
    return a;
  };
  const or = (): Node => {
    let a = and();
    while (peek() === '||') {
      next();
      a = { t: 'or', a, b: and() };
    }
    return a;
  };
  return or();
}

function evalNode(n: Node, ctx: Ctx): unknown {
  switch (n.t) {
    case 'lit':
      return n.v;
    case 'key':
      return ctx(n.k);
    case 'not':
      return !evalNode(n.e, ctx);
    case 'and':
      return evalNode(n.a, ctx) && evalNode(n.b, ctx);
    case 'or':
      return evalNode(n.a, ctx) || evalNode(n.b, ctx);
    case 'eq':
      return evalNode(n.a, ctx) == evalNode(n.b, ctx);
    case 'ne':
      return evalNode(n.a, ctx) != evalNode(n.b, ctx);
  }
}

const cache = new Map<string, Node>();

export function evaluateWhen(expr: string | undefined, ctx: Ctx): boolean {
  if (!expr || !expr.trim()) return true;
  let n = cache.get(expr);
  if (!n) {
    n = parse(tokenize(expr));
    cache.set(expr, n);
  }
  return Boolean(evalNode(n, ctx));
}
