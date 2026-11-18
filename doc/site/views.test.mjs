// The site's unit tests that need no page: `node doc/site/views.test.mjs` runs every doc/site/tests/*.test.mjs
// (tests/run.mjs; `node doc/site/tests/run.mjs forest` runs the files whose name holds `forest`). New tests go into the
// themed file there, as `test(name, fn)` with a one-line intuition and the bug it pins.
import { main } from './tests/run.mjs';

process.exitCode = (await main(process.argv.slice(2))) ? 1 : 0;
