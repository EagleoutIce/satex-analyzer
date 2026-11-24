// The first visit (kinds/cold.mjs): a fresh browser reaches the default example's findings within `ms`.
export default [
  { kind: 'cold', name: 'cold-first-findings', ms: 10000, still: 15000 },
  // every example opened, then built by the browser's pdfTeX (compile server blocked): no request leaves the site
  { kind: 'offline', name: 'examples-offline', ms: 300000,
    steps: 'each example clicked, its findings awaited, Compile pressed with no compile server: no request to another host, none "newly fetched from" one' },
];
