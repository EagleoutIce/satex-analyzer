// The pages' loose DOM for `tsc -p jsconfig.json`: an event's target is an
// element, and a looked-up element may be a form field, a dialog or a details.
interface Loose {
  closest(selector: string): HTMLElement | null; dataset: DOMStringMap; style: CSSStyleDeclaration; hidden: boolean;
  offsetTop: number; offsetHeight: number; offsetWidth: number; parentElement: HTMLElement | null; textContent: string | null;
  classList: DOMTokenList; onclick: ((e: any) => unknown) | null; focus(): void; click(): void;
  value: string; checked: boolean; disabled: boolean; open: boolean; src: string; rows: number;
  selectionStart: number; selectionEnd: number; setSelectionRange(start: number, end: number): void;
  showModal(): void; close(): void; select(): void; max: number | string; id: string; options: HTMLOptionsCollection;
  matches(selector: string): boolean; getBoundingClientRect(): DOMRect; content: string; cells: HTMLCollectionOf<HTMLTableCellElement>;
  onmouseenter: ((e: MouseEvent) => unknown) | null; onmouseleave: ((e: MouseEvent) => unknown) | null; tHead: HTMLTableSectionElement; tBodies: HTMLCollectionOf<HTMLTableSectionElement>;
}
interface EventTarget extends Loose {}
interface Element extends Loose {}
interface Node { data?: string; replaceWith?(...nodes: (Node | string)[]): void; classList?: DOMTokenList }
// What the pages share through `window`.
interface Window { satexTypeList: any; SATEX_FILES: any; satexTypes: any; satexCards: any; siteCM: any; siteTex: any; siteDark: any; satexHealth: any; satexKill: any; satexLayers: any; satexStore: any }
interface Uint8ArrayConstructor { fromBase64?(text: string): Uint8Array }
