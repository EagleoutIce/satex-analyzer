// Snapshots of source (src/lsp/snapshot.rs): calls, key lists, environments as the server read them.
// A view edits their values here and sends them back (`satex/dialogApply`, kind `snapshot`);
// the server prints them. No LaTeX is written in a view.
export {};
declare global {
  /** A key of a key list; `value` null is a bare key. */
  interface SnapKey { key: string; value: string | null }
  /** An argument as the server read it. */
  interface SnapArg { open: string; inner: unknown; text: string; keys?: SnapKey[]; bare?: string[] }
  /** A call or environment of the source as the server read it. */
  interface SnapNode {
    kind: string; name: string; star: boolean; args: SnapArg[]; range: unknown; text: string; at?: unknown; line?: boolean; drop?: boolean;
    package?: { name: string; options: string } | null; body?: string; [field: string]: unknown;
  }
  /** What a view hands the snapshot code: the server's printer and the edit applier. */
  interface SnapCtx {
    print(payload: unknown): Promise<{ edits?: unknown; notes?: string[]; error?: string } | undefined>;
    apply(edits: unknown, kept?: unknown): void;
  }
  interface Window {
    satexSnapshot: {
      list(node: SnapNode): SnapKey[] | null; get(node: SnapNode, key: string): string | null | undefined;
      set<N extends { args: SnapArg[] }>(node: N, key: string, value?: string | null): N;
      setKeys<N extends { args: SnapArg[] }>(node: N, drop: string[], keys: [string, string | null][]): N;
      text(node: SnapNode, open: string, i?: number): string | undefined;
      keys(open: string, kv?: [string, string | null][]): SnapArg;
      arg(open: string, t: string): SnapArg;
      call(name: string, args: SnapArg[], at: unknown, package_?: { name: string; options: string } | null): SnapNode;
      drop<N extends object>(node: N): N & { drop: true }; same(a: unknown, b: unknown): boolean;
      fill(form: { nodes: SnapNode[]; fills: [string, number, number | null, string | null][] }, vals: Record<string, string>): SnapNode[];
      write(ctx: SnapCtx, nodes: SnapNode[], say: (m: string) => void, kept?: unknown): Promise<void>;
    };
  }
}
(() => {
  /** The first key list of `node`. */
  const list = (node: { args: SnapArg[] }) => node.args.find((a) => a.keys)?.keys ?? null;
  /** The value of `key` (null: written bare), undefined when not written. */
  const get = (node: SnapNode, key: string) => list(node)?.find((k) => k.key === key)?.value;
  /** `key` set to `value` (null: bare) in place, else added at the end. */
  function set<N extends { args: SnapArg[] }>(node: N, key: string, value: string | null = null): N {
    const keys = list(node)!, k = keys.find((x) => x.key === key);
    if (k) k.value = value; else keys.push({ key, value });
    return node;
  }
  /** The keys `drop` removed and `keys` (`[key, value]`, '' or null bare) set: kept keys stay in place. */
  function setKeys<N extends { args: SnapArg[] }>(node: N, drop: string[], keys: [string, string | null][]): N {
    const keep = new Set(keys.map(([k]) => k)), l = list(node)!;
    l.splice(0, l.length, ...l.filter((k) => !drop.includes(k.key) || keep.has(k.key)));
    for (const [k, v] of keys) set(node, k, v === '' ? null : v);
    return node;
  }
  /** The text of the `i`th argument opened by `open`. */
  const text = (node: SnapNode, open: string, i = 0) => node.args.filter((a) => a.open === open)[i]?.text;
  /** A key-list argument, a text argument. */
  const keys = (open: string, kv: [string, string | null][] = []): SnapArg => ({ open, inner: null, keys: kv.map(([key, value]) => ({ key, value: value === '' ? null : value })), text: '' });
  const arg = (open: string, t: string): SnapArg => ({ open, inner: null, text: t });
  /** A new call at `at` on a line of its own; `package` loads it when the run does not define it. */
  const call = (name: string, args: SnapArg[], at: unknown, package_: { name: string; options: string } | null = null): SnapNode => ({ kind: 'call', name, star: false, args, range: null, text: '', at, line: true, drop: false, package: package_ });
  /** `node` to be removed. */
  const drop = <N extends object>(node: N) => ({ ...node, drop: true as const });
  /** Whether two snapshots differ in what the printer writes. */
  const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
  /** A form's `nodes` with each field's value where `fills` puts it (`[field, node, arg, key]`): no arg, the
   * body; a key, its value in that key list, dropped when empty; else the argument's text. */
  function fill({ nodes, fills }: { nodes: SnapNode[]; fills: [string, number, number | null, string | null][] }, vals: Record<string, string>): SnapNode[] {
    const out = structuredClone(nodes);
    for (const [f, n, a, k] of fills) {
      const v = vals[f] ?? '', node = out[n];
      if (a == null) node.body = v;
      else if (k == null) node.args[a].text = v;
      else if (v.trim()) set({ args: [node.args[a]] }, k, v);
      else node.args[a].keys = node.args[a].keys!.filter((x) => x.key !== k);
    }
    return out;
  }
  /** `nodes` printed by the server (`ctx.print`) and written (`kept`: what a manager shows again); else `say` gets its notes. */
  const write = (ctx: SnapCtx, nodes: SnapNode[], say: (m: string) => void, kept?: unknown) => ctx.print(nodes).catch((e: Error): { edits?: unknown; notes?: string[] } => ({ notes: [String(e?.message ?? e)] }))
    .then((r) => { if ((r?.edits as unknown[] | undefined)?.length) ctx.apply(r!.edits, kept); else say((r?.notes ?? []).join('; ') || 'no changes'); });
  self.satexSnapshot = { list, get, set, setKeys, text, keys, arg, call, drop, same, fill, write };
})();
