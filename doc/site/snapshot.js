// Snapshots of source (src/lsp/snapshot.rs): calls, key lists, environments as the server read them.
// A view edits their values here and sends them back (`satex/dialogApply`, kind `snapshot`);
// the server prints them. No LaTeX is written in a view.
(() => {
  /** The first key list of `node`. */
  const list = (node) => node.args.find((a) => a.keys)?.keys ?? null;
  /** The value of `key` (null: written bare), undefined when not written. */
  const get = (node, key) => list(node)?.find((k) => k.key === key)?.value;
  /** `key` set to `value` (null: bare) in place, else added at the end. */
  function set(node, key, value = null) {
    const keys = list(node), k = keys.find((x) => x.key === key);
    if (k) k.value = value; else keys.push({ key, value });
    return node;
  }
  /** The keys `drop` removed and `keys` (`[key, value]`, '' or null bare) set: kept keys stay in place. */
  function setKeys(node, drop, keys) {
    const keep = new Set(keys.map(([k]) => k)), l = list(node);
    l.splice(0, l.length, ...l.filter((k) => !drop.includes(k.key) || keep.has(k.key)));
    for (const [k, v] of keys) set(node, k, v === '' ? null : v);
    return node;
  }
  /** The text of the `i`th argument opened by `open`. */
  const text = (node, open, i = 0) => node.args.filter((a) => a.open === open)[i]?.text;
  /** A key-list argument, a text argument. */
  const keys = (open, kv = []) => ({ open, inner: null, keys: kv.map(([key, value]) => ({ key, value: value === '' ? null : value })), text: '' });
  const arg = (open, t) => ({ open, inner: null, text: t });
  /** A new call at `at` on a line of its own; `package` loads it when the run does not define it. */
  const call = (name, args, at, package_ = null) => ({ kind: 'call', name, star: false, args, range: null, text: '', at, line: true, drop: false, package: package_ });
  /** `node` to be removed. */
  const drop = (node) => ({ ...node, drop: true });
  /** Whether two snapshots differ in what the printer writes. */
  const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
  /** A form's `nodes` with each field's value where `fills` puts it (`[field, node, arg, key]`): no arg, the
   * body; a key, its value in that key list, dropped when empty; else the argument's text. */
  function fill({ nodes, fills }, vals) {
    const out = structuredClone(nodes);
    for (const [f, n, a, k] of fills) {
      const v = vals[f] ?? '', node = out[n];
      if (a == null) node.body = v;
      else if (k == null) node.args[a].text = v;
      else if (v.trim()) set({ args: [node.args[a]] }, k, v);
      else node.args[a].keys = node.args[a].keys.filter((x) => x.key !== k);
    }
    return out;
  }
  /** `nodes` printed by the server (`ctx.print`) and written (`kept`: what a manager shows again); else `say` gets its notes. */
  const write = (ctx, nodes, say, kept) => ctx.print(nodes).catch((e) => ({ notes: [String(e?.message ?? e)] }))
    .then((r) => { if (r?.edits?.length) ctx.apply(r.edits, kept); else say((r?.notes ?? []).join('; ') || 'no changes'); });
  self.satexSnapshot = { list, get, set, setKeys, text, keys, arg, call, drop, same, fill, write };
})();
