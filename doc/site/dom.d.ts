// The pages' loose DOM for `tsc -p jsconfig.json`: an event's target is an
// element, and a looked-up element may be a form field, a dialog or a details.
interface Loose {
  [prop: string]: any; // form-field, dialog, media members and the fields the pages hang on elements
  closest(selector: string): HTMLElement | null;
  satexAsk?: Promise<boolean> | null; satexKeep?: () => void; // a widget's open discard question (dialogs.js `discard`)
}
interface EventTarget extends Loose {}
interface Element extends Loose {}
interface Node { contains(other: EventTarget | null): boolean; data?: string; replaceWith?(...nodes: (Node | string)[]): void; classList?: DOMTokenList }
// What the pages share through `window`.
interface Window { satexTypeList: any; SATEX_FILES: any; satexTypes: any; satexCards: any; siteCM: any; siteTex: any; siteDark: any; satexHealth: any; satexKill: any; [name: `satex${string}`]: any }
interface Event { key?: string; inputType?: string }
interface Navigator { connection?: { saveData?: boolean; effectiveType?: string } }
interface Uint8ArrayConstructor { fromBase64?(text: string): Uint8Array }
