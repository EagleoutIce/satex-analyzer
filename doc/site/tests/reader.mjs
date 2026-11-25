// An independent reader of LaTeX structure for the property tests: it shares no code with pdfedit.ts, so a gate bug
// and a reader bug do not cancel out. It reads braces, commands, environments, comments, math, verbatim and blank lines.

const MATH_ENV = /^(equation|align|gather|multline|eqnarray|displaymath|math)\*?$/;
/** The structure of `t`: brace depth at the end and its lowest point, command names (`[offset, name]`), environment
 * edges, comments, verbatim bodies, math mismatches (`bad`), blank lines inside a group or math (`parIn`; a footnote's or margin note's argument may hold them), scripts
 * outside math, `&` and `#` counts. */
export function read(t) {
  const r = { depth: 0, low: 0, cs: [], env: [], com: [], verb: [], bad: 0, parIn: 0, script: 0, amp: 0, hash: 0 };
  // the open groups, each true when it is a footnote's or margin note's argument (LaTeX reads those `\long`: a blank
  // line may stand in them); `last`: the last command name and where it ended
  let math = '', last = null;
  const open = [];
  for (let i = 0; i < t.length; i++) {
    const c = t[i];
    if (c === '%') { const e = t.indexOf('\n', i); r.com.push(t.slice(i, e < 0 ? t.length : e)); i = e < 0 ? t.length : e - 1; continue; }
    if (c === '\\') {
      const m = /^\\([A-Za-z@]+|[\s\S]?)/.exec(t.slice(i, i + 64)), n = m[1];
      let e = i + m[0].length;
      if (/^[A-Za-z@]/.test(n)) {
        r.cs.push([i, n]);
        last = [n, e];
        if (n === 'verb' && t[e] && !/[\sA-Za-z*]/.test(t[e])) {
          const end = t.indexOf(t[e], e + 1), nl = t.indexOf('\n', e + 1);
          if (end > 0 && (nl < 0 || end < nl)) { r.verb.push(t.slice(e + 1, end)); i = end; continue; }
        }
        const env = (n === 'begin' || n === 'end') && /^\{([^{}\n]*)\}/.exec(t.slice(e));
        if (env) {
          r.env.push(`${n}:${env[1]}`);
          e += env[0].length;
          if (n === 'begin' && env[1] === 'verbatim') {
            const end = t.indexOf('\\end{verbatim}', e);
            r.verb.push(t.slice(e, end < 0 ? t.length : end));
            i = (end < 0 ? t.length : end) - 1;
            continue;
          }
          if (MATH_ENV.test(env[1])) {
            if (n === 'begin') { if (math) r.bad++; else math = env[1]; } else if (math === env[1]) math = ''; else r.bad++;
          }
        }
      } else if (n === '(' || n === '[') { if (math) r.bad++; else math = n; }
      else if (n === ')' || n === ']') { if (math === (n === ')' ? '(' : '[')) math = ''; else r.bad++; }
      i = e - 1;
      continue;
    }
    if (c === '$') {
      const two = t[i + 1] === '$' && (math === '$$' || !math), d = two ? '$$' : '$';
      if (!math) math = d; else if (math === d) math = ''; else r.bad++;
      i += d.length - 1;
      continue;
    }
    if (c === '{') { r.depth++; open.push(!!last && /^(footnote|footnotetext|marginpar)$/.test(last[0]) && /^\s*(\[[^\]]*\]\s*)?$/.test(t.slice(last[1], i))); }
    else if (c === '}') { r.depth--; r.low = Math.min(r.low, r.depth); open.pop(); }
    else if (c === '\n' && /^\n[ \t]*\n/.test(t.slice(i, i + 64)) && ((r.depth > 0 && !(open.length === r.depth && open.every(Boolean))) || math)) r.parIn++;
    else if ((c === '_' || c === '^') && !math) r.script++;
    else if (c === '&') r.amp++;
    else if (c === '#') r.hash++;
  }
  if (math) r.bad++;
  return r;
}
