// What site.js shares through `window`, typed for the files that use it (playground.js, the ported views).
/** A floating card or note (site.js `floating`). */
interface SiteFloat {
  el: HTMLDivElement; show(at: Element | { left: number; top: number; right: number; bottom: number }, html: string, host?: Element | null): void; hide(ms?: number): void; later(f: () => void, ms: number): void;
}
/** The syntax highlighters site.js builds from the vendored CodeMirror. */
interface SiteTex { language: any; bib: any; highlighter: any }
interface SiteCards {
  keyTitle(k: Record<string, any>): string;
  chipList(names: string[], chip: (name: string) => string, shown?: number): string;
  explain(r: Record<string, any>, brief?: boolean): string;
  brief(r: Record<string, any>): string;
  signature(spec: string): string;
  slots(html: string, kinds?: Record<string, string>): string;
  usage(text: string, types?: Record<string, string>): { html: string; kinds: Record<string, any> };
  tex(text: string, link?: boolean): string;
  inline(text: string): string;
  markdown(text: string): string;
  ruleCard(r: Record<string, any>, href?: string, own?: boolean): string;
  placeLink(path: string, line: number, file?: string): string;
  upstream(path: string): unknown;
  csChip(name: string): string;
  csName(name: unknown): string;
  value(v: unknown): string;
  esc(s: unknown): string;
  notation(marked: unknown, sketches?: unknown[], picture?: unknown): string;
  withKeys(marked: unknown, keys?: unknown[], fonts?: unknown[], align?: unknown[], ligatures?: unknown[], lines?: unknown[]): string;
  typesetView(rows: Record<string, any>[], o?: Record<string, any>): string;
  typesetWire(root: HTMLElement): void;
  hover(value: any): any;
  useOf(r: Record<string, any>): string;
  duration(ms: number): string;
}
interface Window {
  siteDark: () => boolean;
  siteCM: { (): Promise<any>; loaded?: Promise<any> };
  siteTex: { (): Promise<SiteTex>; loaded?: Promise<SiteTex> };
  satexCards: SiteCards;
  satexTip: { card: SiteFloat; note: SiteFloat };
  satexProgress: {
    html(done?: number | null, label?: string): string; set(el: HTMLElement, done?: number | null): void; left(done: number, ms: number): number | null;
  };
  satexEmbedButton(tools: Element | null, snippet: string): void;
  satexRule: { named(name: string): Promise<any>; link(code: string): string };
  satexTiles: (data: any[], switches?: any) => string;
  satexTileIcon(name: string): string;
}
