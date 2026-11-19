// The echo on the page's paper per View > Pages mode and site theme (kinds/paper.mjs). The light theme walks every
// mode; the dark theme the modes that change the paper there.
export default [
  { kind: 'paper', name: 'paper-light', theme: 'light' },
  { kind: 'paper', name: 'paper-dark', theme: 'dark', modes: ['original', 'dark', 'sepia'] },
];
