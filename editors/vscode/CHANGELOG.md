# Changelog

## Unreleased

- Code lenses (Edit table…, Edit image…, Edit placement…, Edit drawing…) show without an edit: the server asks the editor to refresh them once its first run is in.
- Another installed copy of SaTeX is detected: a warning offers to uninstall it. A clashing command no longer stops the activation, and a window has one status item.
- Every playground widget opens beside the editor: `satex: Open Widget…`, the Library and the project Settings are new. A dialog's Cancel closes its panel, its image can be saved elsewhere or replaced.
- Trace Expansion Here keeps the trace for stepping: Alt+] and Alt+[ move through its steps, the editor follows each to its line, the status bar names it, and clicking it picks any step. Escape closes the trace.

## 0.1.0

- Initial release: language server client and background kernel cache build.
