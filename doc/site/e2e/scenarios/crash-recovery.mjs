// The analyzer crashing under the editor (kinds/crash.mjs): typed text kept, the restart counted, the backoff.
export default [
  { kind: 'crash', name: 'crash-midrun', how: 'midrun' },
  { kind: 'crash', name: 'crash-panic', how: 'panic' },
  { kind: 'crash', name: 'crash-oom', how: 'oom' },
  { kind: 'crash', name: 'crash-backoff', how: 'backoff' },
];
