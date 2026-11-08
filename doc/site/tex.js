// Compile on a static host: tex-worker.js runs pdfTeX, pdf.js shows what it wrote. The answer has the
// shape of the local server's (src/cli/gui/compile.rs), so the PDF pane reads both alike.
// `parseLog` follows src/cli/compile.rs `parse_log`.

(() => {
  const DPI = 144, BP_PER_INCH = 72;
  const GUI = '<code class="satex-gui">satex gui</code>';
  const escape = (s) => String(s).replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);
  const text = (bytes) => new TextDecoder().decode(bytes);

  /** The file a log line opens with `(path` and closes with `)`, tracked on a stack. */
  function track(line, files) {
    for (let i = 0; i < line.length; i++) {
      if (line[i] === ')') files.pop();
      else if (line[i] === '(') {
        const name = /^[^\s()]*/.exec(line.slice(i + 1))[0];
        files.push(name.includes('.') && /^[./]|\//.test(name) ? name : '');
        i += name.length;
      }
    }
  }
  /** The log's errors and warnings: `{path, line, severity, code, message}`, the paths the project's where known. */
  function parseLog(log, known, main) {
    const out = [], files = [], lines = log.split('\n');
    const own = (p) => { const n = (p ?? '').replace(/^\.\//, ''); return known.has(n) ? n : main; };
    const push = (path, line, error, message) => out.push({ path: own(path), line, severity: error ? 'error' : 'warning', code: error ? 'tex-error' : 'tex-warning', message });
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i], placed = /^(.*?(?:\.tex|\.sty|\.cls|\/[^:]*)):(\d+): (.*)$/.exec(line);
      if (placed && !line.startsWith('(')) {
        let message = placed[3];
        // a package error goes on in lines that start with `(package)`
        while (/^\(.*\)/.test(lines[i + 1] ?? '')) message += ` ${lines[++i].replace(/^[^)]*\)/, '').trim()}`;
        if (!/^(Emergency stop\.|\s*==> Fatal error)/.test(message)) push(placed[1], +placed[2], true, message);
        continue;
      }
      if (/(?:LaTeX|Package \S+|Class \S+) Warning: /.test(line)) {
        let message = line.slice(line.search(/LaTeX|Package |Class /));
        while (/^(\(|  )/.test(lines[i + 1] ?? '')) message += ` ${lines[++i].replace(/^\([^)]*\)/, '').trim()}`;
        // the end-of-run summaries repeat what was said where it arose
        if (message.startsWith('LaTeX Warning: There were') || message.includes('Label(s) may have changed')) continue;
        const at = /on input line (\d+)/.exec(message);
        push(files.findLast(Boolean), at ? +at[1] : null, false, message);
        continue;
      }
      const fatal = line.startsWith('! ') && !out.some((r) => r.severity === 'error') ? line.slice(2) : /^\*\*\* \((.*)\)$/.exec(line)?.[1];
      if (fatal) { push(files.findLast(Boolean), null, true, fatal); continue; }
      if (/^(Over|Under)full \\/.test(line)) { push(files.findLast(Boolean), +(/at lines? (\d+)/.exec(line)?.[1] ?? 0) || null, false, line); continue; }
      track(line, files);
    }
    return out;
  }

  /** A source's name for people: this site, or its host. */
  const hostName = (h) => h === location.host ? 'this site' : h;
  const listed = (names) => names.map((n) => `<code>${escape(n)}</code>`).join(', ');
  /** What a build could not do here, in words, with where it can: the reasons a browser's pdfTeX has. */
  function advice(log, run, engine) {
    const say = [], names = [...new Set([...log.matchAll(/File `([^']+)' not found/g)].map((m) => m[1]))];
    const asked = run.asked.map(hostName).join(', '), dead = run.dead.map(hostName).join(', ');
    if (names.length) say.push(run.asked.length
      ? `The browser's TeX found no ${listed(names)}. It looked in the project, its core tree and the browser's cache, then asked ${escape(asked)}.`
      : `The browser's TeX found no ${listed(names)} in the project, its core tree or the browser's cache. Downloads are off in Settings, under Compile.`);
    // a host down on a probe for a file nobody needs is no news: Commands names it
    if (dead && (names.length || run.nopdf)) say.push(`No answer from ${escape(dead)}${run.failed.length ? `, which may hold ${listed(run.failed)}` : ''}.`);
    if (/requires either XeTeX or|requires XeTeX|requires LuaTeX|LuaTeX or XeTeX/.test(log) || (engine && engine !== 'pdftex')) say.push(`The browser's TeX is pdfTeX only${engine && engine !== 'pdftex' ? `, this project runs on ${escape(engine)}` : ''}.`);
    if (/shell-escape|shell escape|write18 enabled/i.test(log) && /Error/.test(log)) say.push('The browser\'s TeX runs no programs: a document that needs shell escape stops or lacks its output.');
    if (run.biblatex) say.push('The bibliography needs biber, which the browser\'s TeX lacks: its citations stay unresolved.');
    return say.length ? `${say.join(' ')} ${GUI} builds with your own TeX and packages.` : '';
  }

  /** pdf.js, once. */
  let pdfjs = null;
  function pdfLib(tex) {
    return pdfjs ??= import(tex.pdfjs).then((lib) => {
      lib.GlobalWorkerOptions.workerSrc = tex.pdfWorker;
      return lib;
    });
  }
  const png = (canvas) => new Promise((done) => canvas.toBlob(done, 'image/png'));
  /** A page as a PNG blob at `DPI`. */
  async function draw(page) {
    const viewport = page.getViewport({ scale: DPI / BP_PER_INCH }), canvas = document.createElement('canvas');
    canvas.width = Math.ceil(viewport.width);
    canvas.height = Math.ceil(viewport.height);
    await page.render({ canvas, canvasContext: canvas.getContext('2d'), viewport }).promise;
    return png(canvas);
  }
  /** A page's words as `pdftotext -bbox` gives them: `[page, x0, y0, x1, y1, text]` in bp from the top left. */
  async function wordsOf(page, number) {
    const [x0, , , y1] = page.view, out = [];
    for (const item of (await page.getTextContent()).items) {
      if (!item.str?.trim()) continue;
      const x = item.transform[4] - x0, y = y1 - item.transform[5], each = item.width / item.str.length;
      for (const m of item.str.matchAll(/\S+/g)) out.push([number, x + m.index * each, y - item.height * 0.8, x + (m.index + m[0].length) * each, y + item.height * 0.2, m[0]]);
    }
    return out;
  }
  /** A page's links: `{page, rect, uri}` or `{page, rect, to}`, in fractions of their page from the top left. */
  async function linksOf(doc, page, number) {
    const [x0, y0, x1, y1] = page.view, w = x1 - x0, h = y1 - y0, out = [];
    for (const a of await page.getAnnotations({ intent: 'display' })) {
      if (a.subtype !== 'Link' || !a.rect) continue;
      const [l, b, r, t] = a.rect, rect = [(l - x0) / w, (y1 - t) / h, (r - l) / w, (t - b) / h];
      // `url` is set only for a safe scheme, `unsafeUrl` is the address as written
      if (a.url) { out.push({ page: number, rect, uri: a.unsafeUrl ?? a.url }); continue; }
      try {
        const d = typeof a.dest === 'string' ? await doc.getDestination(a.dest) : a.dest;
        if (!d) continue;
        const to = await doc.getPage(await doc.getPageIndex(d[0]) + 1), [vx0, vy0, vx1, vy1] = to.view, kind = d[1]?.name;
        const [x, y] = kind === 'XYZ' ? [d[2], d[3]] : /^Fit(B?H)$/.test(kind ?? '') ? [null, d[2]] : kind === 'FitR' ? [d[2], d[5]] : [null, null];
        out.push({ page: number, rect, to: { page: to.pageNumber, x: ((x ?? vx0) - vx0) / (vx1 - vx0), y: Math.max(0, (vy1 - (y ?? vy1)) / (vy1 - vy0)) } });
      } catch {} // a link to nowhere is no link
    }
    return out;
  }
  /** The PDF's pages as images, words, links and labels. */
  async function read(tex, bytes, tell) {
    const lib = await pdfLib(tex), task = lib.getDocument({ data: bytes.slice(), isEvalSupported: false }), doc = await task.promise;
    const pages = [], blobs = {}, wordList = [], linkList = [];
    for (let n = 1; n <= doc.numPages; n++) {
      tell(`rendering page ${n} of ${doc.numPages}`);
      const page = await doc.getPage(n), name = `page-${n}.png`;
      blobs[name] = URL.createObjectURL(await draw(page));
      pages.push(name);
      wordList.push(...await wordsOf(page, n));
      linkList.push(...await linksOf(doc, page, n));
      page.cleanup();
    }
    const labels = await doc.getPageLabels().catch(() => null);
    task.destroy();
    return { pages, blobs, wordList, linkList, pagelabels: (labels ?? []).map((label, index) => ({ index, label: { '/P': label } })) };
  }

  /** The worker, kept between builds: it holds the engine and what it has fetched. */
  let worker = null;
  const MIB = 1048576;
  function ask(message, tell) {
    worker ??= new Worker(message.tex.worker);
    return new Promise((done, fail) => {
      worker.onerror = (e) => fail(new Error(e.message || 'the TeX worker stopped'));
      worker.onmessage = ({ data }) => {
        if (data.progress) tell(`downloading ${data.progress.label}: ${Math.round(data.progress.loaded / MIB)} of ${Math.round(data.progress.size / MIB)} MiB`);
        else if (data.fetching) tell(`fetching ${data.fetching}`);
        else if (data.phase) tell(data.phase);
        else if (data.error) fail(new Error(data.error));
        else done(data);
      };
      worker.postMessage(message);
    });
  }

  /** Builds `main` of `files`, asking the sources `from`: `{answer, gz}`, the pane's answer and the SyncTeX data for its worker to parse. */
  async function build(tex, files, main, engine, tell, from) {
    const run = await ask({ tex, files, main, from }, tell);
    const by = Object.fromEntries(run.outputs.map((o) => [o.name, o.bytes]));
    const decoded = {};
    for (const n of Object.keys(by)) if (/\.(log|blg|ilg)$/.test(n)) decoded[n] = text(by[n]);
    const log = decoded[`${run.job}.log`] ?? '';
    const dir = main.includes('/') ? main.slice(0, main.lastIndexOf('/') + 1) : '';
    const records = parseLog(log, new Set(Object.keys(files).map((n) => n.startsWith(dir) ? n.slice(dir.length) : n)), main.slice(dir.length)).map((r) => ({ ...r, path: dir + r.path }));
    const shown = by[`${run.job}.pdf`] ? await read(tex, by[`${run.job}.pdf`], tell) : { pages: [], blobs: {}, wordList: [], linkList: [], pagelabels: [] };
    const blobs = { ...shown.blobs };
    for (const [name, bytes] of Object.entries(by)) blobs[name] = URL.createObjectURL(new Blob([bytes], { type: name.endsWith('.pdf') ? 'application/pdf' : /\.(log|aux|toc|lof|lot|out|bbl|blg|ilg|ind|idx)$/.test(name) ? 'text/plain' : 'application/octet-stream' }));
    const answer = {
      browser: true, pdf: by[`${run.job}.pdf`] ? `${run.job}.pdf` : null, dpi: DPI, records, logs: decoded, stderr: '',
      pages: shown.pages, wordList: shown.wordList, linkList: shown.linkList, pagelabels: shown.pagelabels, blobs,
      commands: run.commands, from: Object.entries(run.from).map(([h, n]) => [hostName(h), n]), asked: run.asked.map(hostName), dead: run.dead.map(hostName), outputs: run.outputs.map((o) => ({ name: o.name, size: o.bytes.length })),
      note: advice(log, { ...run, nopdf: !by[`${run.job}.pdf`] }, engine),
    };
    return { answer, gz: by[`${run.job}.synctex.gz`] ?? null };
  }

  /** The worker goes with what it holds: the next build starts it again. */
  function stop() {
    worker?.terminate();
    worker = null;
  }
  /** A PDF's first page as an image URL, for the image dialog. */
  async function firstPage(tex, bytes) {
    const lib = await pdfLib(tex), task = lib.getDocument({ data: bytes.slice(), isEvalSupported: false });
    const blob = await draw(await (await task.promise).getPage(1));
    task.destroy();
    return URL.createObjectURL(blob);
  }
  self.satexTex = { build, stop, firstPage, parseLog, advice, hostName };
})();
