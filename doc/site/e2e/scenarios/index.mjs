// Every scenario, by area (one file each), and the tiers. `quick` holds the behaviors a push must not break, within
// about five minutes; `full` is every scenario; `perf` the typing scenarios whose timing bounds (echo draw p95, key to
// source) count as failures there. A name is unique across areas.
import typingPdf from './typing-pdf.mjs';
import titles from './titles-footnotes.mjs';
import math from './math.mjs';
import clicks from './clicks.mjs';
import select from './select.mjs';
import tints from './tints-typeset.mjs';
import widgets from './widgets.mjs';
import completion from './completion.mjs';
import dbl from './double-click.mjs';
import foot from './footer-blank.mjs';
import newpara from './echo-paragraphs.mjs';
import follow from './follow-highlight.mjs';
import dark from './dark-mode.mjs';
import tikz from './tikz-actions.mjs';
import refactor from './refactor.mjs';
import crash from './crash-recovery.mjs';
import editor from './editor.mjs';
import fuzz from './fuzz.mjs';
import sync from './sync.mjs';
import scratch from './scratch.mjs';
import cold from './cold-start.mjs';
import insert from './insert.mjs';
import robust from './robust.mjs';
import draw from './draw.mjs';
import landing from './landing.mjs';
import user from './user.mjs';
import big from './big.mjs';
import swap from './swap.mjs';

const AREAS = { 'typing-pdf': typingPdf, 'titles-footnotes': titles, math, clicks, select, 'tints-typeset': tints, widgets,
  completion, 'double-click': dbl, 'footer-blank': foot, 'echo-paragraphs': newpara, 'follow-highlight': follow,
  'dark-mode': dark, 'tikz-actions': tikz, refactor, 'crash-recovery': crash, editor, fuzz, sync, scratch, 'cold-start': cold, insert, robust, draw, landing, user, big, swap };

export const SCENARIOS = Object.entries(AREAS).flatMap(([area, list]) => list.map((sc) => ({ area, ...sc })));

export const TIERS = {
  quick: ['title-human', 'prose-burst', 'caption', 'pagebreak', 'fz-thanks-mark', 'click-footnote', 'click-macro-start',
    'math-display-preview', 'select-backspace', 'tint-bang', 'col-align', 'forest-child', 'tc-src-ref', 'dbl-pdf-tikz',
    'plain-before-number', 'newpara-pdf', 'newpara-ts', 'after-macro-pdf', 'gutter-marker', 'close-untouched-page', 'close-edited-x'],
  perf: ['prose-burst', 'title-human', 'caption', 'burst-3rows', 'math-burst', 'bold', 'cold-first-findings', 'book-300-pages'],
};

const seen = new Set();
for (const sc of SCENARIOS) { if (seen.has(sc.name)) throw new Error(`two scenarios named ${sc.name}`); seen.add(sc.name); }
for (const [tier, names] of Object.entries(TIERS)) for (const n of names) if (!seen.has(n)) throw new Error(`tier ${tier}: no scenario ${n}`);
