// A build's pages replacing the shown ones while a person types (kinds/swap.mjs): no blank or dimmed page, no older
// build after a newer one, no scroll, no layer gone in any frame.
/** Three pages: sections of plain paragraphs, a macro's text (origins), a list (envs), a finding (an undefined reference). */
const para = (i) => `Paragraph ${i} of the text carries plain words set by \\model{} over a few rows, so that a page holds several of them and an edit above moves the rows below it; as part ${i % 10 === 2 ? 'of Section~\\ref{sec:none}' : i} says.`;
export const PAGES = `\\documentclass{article}\n\\newcommand{\\model}{TinyNet}\n\\begin{document}\n${[1, 2, 3, 4].map((s) => `\\section{Part ${s}}\\label{sec:p${s}}\n${[1, 2, 3, 4, 5, 6, 7].map((i) => para(s * 10 + i)).join('\n\n')}\n\\begin{itemize}\n\\item one item of part ${s}\n\\item another item\n\\end{itemize}`).join('\n')}\n\\end{document}\n`;
const LAYERS = ['origins', 'envs', 'findings'], CLASSES = ['pdf-orig', 'pdf-env', 'pdf-finding'];
export default [
  { kind: 'swap', name: 'swap-editor-typing', source: PAGES, example: 'hello.tex', layers: LAYERS, layerClasses: CLASSES, at: 'Paragraph 12 of the text', keys: [' quokka zebu wombat', 'Build', ' emu koala dingo numbat', 'Build', ' bilby possum', 'Build', ' echidna platypus'], minBuilds: 2,
    steps: 'three pages with the origins, envs and findings layers on; type three bursts into a paragraph of page 1 in the editor, builds land between: every frame shows the pages drawn and undimmed, the layers on them, the same scroll' },
  { kind: 'swap', name: 'swap-editor-below', source: PAGES, example: 'hello.tex', layers: LAYERS, layerClasses: CLASSES, at: 'Paragraph 11 of the text', scrollTo: 2, follow: false, keys: [' quokka zebu wombat emu koala dingo numbat bilby possum echidna platypus wallaby', 'Build', ' kangaroo bandicoot', 'Build', ' cassowary dugong galah'], minBuilds: 2,
    steps: 'page 2 in view, typing on page 1 pushes its rows: page 2 is redrawn by each build, never blank, dimmed or scrolled' },
  { kind: 'swap', name: 'swap-pdf-typing', source: PAGES, example: 'hello.tex', layers: LAYERS, layerClasses: CLASSES, via: 'pdf', word: { word: 'rows,', nth: 1 }, keys: [' quokka zebu wombat', 'Pause', ' emu koala dingo numbat', 'Pause', ' bilby possum', 'Pause', ' echidna platypus'], minBuilds: 2,
    steps: 'Type mode: the same bursts typed into the page: the layers stay under the echo, the pages swap without a blank' },
  { kind: 'swap', name: 'swap-grow-page', source: PAGES, example: 'hello.tex', layers: LAYERS, layerClasses: CLASSES, at: 'another item\n\\end{itemize}\n\\section{Part 4}', k: 12, keys: ['Enter', '\\item more', 'Pause', 'Enter', '\\item still more', 'Pause'], minBuilds: 2, scrollTo: 2,
    steps: 'items added line by line: builds of a text with one more row each land, the pages in view keep their place and layers' },
  { kind: 'swap', name: 'swap-failed-build', source: PAGES, example: 'hello.tex', layers: LAYERS, layerClasses: CLASSES, at: '\\newcommand{\\model}', k: 0, keys: ['\\usepackage{', 'Pause', 'graphic', 'Pause', 'x'], pauseMs: 2500, minBuilds: 1,
    steps: 'a package name typed into the preamble: the builds of the half name stop before any page (no PDF), the last good pages and their layers stay shown under the failure\'s line, then the build of the whole name replaces them' },
];
