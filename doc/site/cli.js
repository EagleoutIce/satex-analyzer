// The satex command line's help from clap's command tree (`satex completions json`): what a word of
// a command line is, as a card. site.js shows it on the docs' command lines, playground.js on its console.
(() => {
  const esc = (s) => String(s ?? '').replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);
  const words = (line) => [...line.matchAll(/\S+/g)].map((m) => ({ text: m[0], from: m.index, to: m.index + m[0].length }));
  /** What the word at `offset` of `line` is: a subcommand, a flag, a flag's value, a query or an argument, with its
   * node in `tree`. A leading `$` and `satex` are skipped; `null` off every word or on an unknown flag. */
  function at(tree, line, offset) {
    const ws = words(line);
    if (ws[0]?.text === '$') ws.shift();
    let node = tree.command, waiting = null;
    for (const [i, w] of ws.entries()) {
      let hit;
      if (i === 0 && w.text === 'satex') hit = { kind: 'command', node, path: 'satex' };
      else if (waiting) { hit = { kind: 'value', node, arg: waiting, value: w.text }; waiting = null; }
      else if (w.text.startsWith('-') && w.text !== '-') {
        const [name, ...value] = w.text.split('=');
        const arg = node.args.find((a) => (name.startsWith('--') ? a.long && `--${a.long}` === name : a.short && `-${a.short}` === name));
        hit = arg && (value.length ? { kind: 'value', node, arg, value: value.join('=') } : { kind: 'flag', node, arg });
        if (arg?.takes_value && !value.length) waiting = arg;
      } else {
        const sub = node.subcommands.find((c) => c.name === w.text);
        if (sub) node = sub;
        hit = sub ? { kind: 'command', node }
          : node.name === 'query' && tree.queries?.includes(w.text) ? { kind: 'query', node, value: w.text }
            : { kind: 'argument', node, arg: node.args.find((a) => a.positional), value: w.text };
      }
      if (offset >= w.from && offset < w.to) return hit ? { ...hit, from: w.from, to: w.to } : null;
    }
    return null;
  }
  /** A flag's value: its name, the values it takes, its default. */
  const takes = (a) => [
    a.value_names?.length && `<code>${esc(a.value_names.map((v) => `<${v}>`).join(' '))}</code>`,
    a.possible_values?.length && `one of ${a.possible_values.map((v) => `<code>${esc(v)}</code>`).join(' ')}`,
    a.defaults?.length && `default <code>${esc(a.defaults.join(','))}</code>`,
  ].filter(Boolean).join(' · ');
  /** `hit` as a card, in the shape of the control sequence cards (`.cs-card`). */
  function card(hit, about = {}) {
    if (!hit) return '';
    // the page that documents it: a query its section, `satex lint` the lints, any other command the CLI page
    const root = new URL('.', document.querySelector('script[src*="cli"]')?.src ?? location.href).href;
    const docs = (kind, name) => kind === 'query' ? `wiki/queries.html#${name}` : /^satex (query|trace)\b/.test(name) ? `wiki/queries.html${name.endsWith('trace') ? '#trace' : ''}` : /^satex lint\b/.test(name) ? 'wiki/lints.html' : 'wiki/cli.html';
    const head = (name, kind) => {
      const href = kind === 'query' || kind === 'subcommand' ? new URL(docs(kind, name), root).href : null;
      return `<div class="cs-head">${href ? `<a href="${esc(href)}" title="its documentation"><b>${esc(name)}</b></a>` : `<b>${esc(name)}</b>`}<span class="cs-chip cs-k">${esc(kind)}</span></div>`;
    };
    const p = (text) => (text ? `<p>${esc(text).replace(/`([^`]+)`/g, '<code>$1</code>')}</p>` : '');
    const a = hit.arg;
    if (hit.kind === 'command') return `<div class="cs-card">${head(hit.path ?? `satex ${hit.node.name}`, 'subcommand')}${p(hit.node.about)}</div>`;
    if (hit.kind === 'query') return `<div class="cs-card">${head(hit.value, 'query')}${p(about[hit.value] ?? 'A query: `satex query --list` names them all.')}</div>`;
    if (!a) return '';
    const name = a.positional ? `<${(a.value_names ?? [a.id]).join(' ')}>` : [a.long && `--${a.long}`, a.short && `-${a.short}`].filter(Boolean).join(', ');
    const kind = hit.kind === 'flag' ? (a.takes_value ? 'option' : 'flag') : a.positional ? 'argument' : `value of --${a.long ?? a.short}`;
    const value = hit.kind === 'value' && a.possible_values?.length && !a.possible_values.includes(hit.value) ? p(`\`${hit.value}\` is none of the values it takes.`) : '';
    return `<div class="cs-card">${head(name, kind)}${a.takes_value || a.positional ? `<div class="cs-sig">${takes(a) || '<code>a value</code>'}</div>` : ''}${p(a.help)}${value}</div>`;
  }
  self.satexCli = { at, card };
})();
