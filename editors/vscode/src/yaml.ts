// A dotted key of satex.yaml, read and written in place: block mappings only, the rest of the
// text (comments, order, other keys) kept. No `vscode` import: unit tests load it.

const indentOf = (line: string) => line.length - line.trimStart().length;
const blank = (line: string) => /^\s*(#.*)?$/.test(line);
const keyRe = (key: string) => new RegExp(`^(\\s*)${key.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*:(.*)$`);

/** Where `path` is: the line of each key found, and the scope (`[from, to)`, indent) the next key would go in. */
function locate(lines: string[], path: string[]): { found: number[]; from: number; to: number; indent: number } {
    let [from, to, parent] = [0, lines.length, -1];
    const found: number[] = [];
    for (const key of path) {
        const kids = lines.slice(from, to).filter(l => !blank(l)).map(indentOf).filter(i => i > parent);
        const indent = kids.length ? Math.min(...kids) : parent + 2;
        const at = lines.findIndex((l, i) => i >= from && i < to && indentOf(l) === indent && keyRe(key).test(l));
        if (at < 0) { return { found, from, to, indent: parent < 0 ? 0 : indent }; }
        found.push(at);
        let end = at + 1;
        while (end < to && (blank(lines[end]) || indentOf(lines[end]) > indent)) { end++; }
        [from, to, parent] = [at + 1, end, indent];
    }
    return { found, from, to, indent: parent };
}

/** The value written at the dotted `key`, as text, else undefined. */
export function read(text: string, key: string): string | undefined {
    const lines = text.split('\n'), path = key.split('.');
    const { found } = locate(lines, path);
    if (found.length < path.length) { return undefined; }
    const value = keyRe(path[path.length - 1]).exec(lines[found[found.length - 1]])?.[2].replace(/\s+#.*$/, '').trim();
    return value || undefined;
}

/** A value as YAML writes it: plain where it reads back the same, else JSON (YAML's flow style). */
export function scalar(value: unknown): string {
    if (typeof value !== 'string') { return JSON.stringify(value); }
    return /^[\w./-]+$/.test(value) && !/^([-+]?[\d.]+|true|false|null|yes|no|on|off)$/i.test(value) ? value : JSON.stringify(value);
}

/** A typed value as YAML text: by the key's type. */
export function yamlValue(type: string, text: string): string {
    if (type === 'boolean') { return String(text === 'true'); }
    if ((type === 'number' || type === 'integer') && text.trim() !== '' && !isNaN(Number(text))) { return String(Number(text)); }
    if (type === 'array') {
        try { return scalar(JSON.parse(text)); } catch { return scalar(text.split(',').map(s => s.trim()).filter(Boolean)); }
    }
    return scalar(text);
}

/** `text` with the dotted `key` set to `value` (YAML text), the keys it lacks added at their place. */
export function write(text: string, key: string, value: string): string {
    const lines = text === '' ? [] : text.replace(/\n$/, '').split('\n'), path = key.split('.');
    const { found, to, indent } = locate(lines, path);
    if (found.length === path.length) {
        const at = found[found.length - 1], comment = /\s+#.*$/.exec(lines[at].split(':').slice(1).join(':'))?.[0] ?? '';
        lines[at] = `${' '.repeat(indentOf(lines[at]))}${path[path.length - 1]}: ${value}${comment}`;
    } else {
        // a key holding a scalar becomes the mapping of the rest
        if (found.length) {
            const at = found[found.length - 1];
            lines[at] = `${' '.repeat(indentOf(lines[at]))}${path[found.length - 1]}:`;
        }
        let end = to;
        while (end > (found.length ? found[found.length - 1] + 1 : 0) && blank(lines[end - 1] ?? 'x')) { end--; }
        const rest = path.slice(found.length).map((k, i, all) => `${' '.repeat(indent + 2 * i)}${k}:${i === all.length - 1 ? ` ${value}` : ''}`);
        lines.splice(end, 0, ...rest);
    }
    return `${lines.join('\n')}\n`;
}
