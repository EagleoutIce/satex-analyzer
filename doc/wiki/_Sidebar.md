**[<img src="https://raw.githubusercontent.com/EagleoutIce/satex-analyzer/main/doc/wiki/img/satex-wordmark.svg" alt="SaTeX" height="20"> Home](Home)**

**[🔍 Lints](Lints)**
- [chktex compliance](Chktex-Coverage)

**[❓ Queries](Queries)**
- [Explain](Queries#explain)
- [Slice](Queries#slice)
- [JSON requests](Queries#json-requests)

**[⌨️ Command line](Cli)**
- [Shell completion](Cli#shell-completion)

**[⚙️ Configuration](Configuration)**
- [Profiles](Configuration#profiles)
- [Plugins](Configuration#plugins)

**[🧩 Editors](Lsp)**
- [VS Code](Vscode)
- [IntelliJ](Intellij)
- [Neovim, TeXstudio, any LSP client](Lsp#editor-setup)
- [What the language server offers](Lsp#capabilities)
- [Type plugins](Type-plugins)
  - [Writing a type plugin](Writing-type-plugins)

**[📖 Semantics](Semantics)**
- [Why TeX is hard](Challenges)
- [What TeX does](Semantics)
  - [Where satex is not exact](Semantics#14-where-satex-is-not-exact)
  - [Caches](Semantics#15-caches)
- [Abstract domains](Domains)
  - [`Value` and `OneOf`](Domains#value-what-a-control-sequence-or-register-holds)
  - [Boxes and paragraphs](Domains#modes-and-box-dimensions)
  - [Unknown text](Domains#unknown-text-the-textclass-lattice)
  - [Deferred material: writes, floats, inserts](Domains#files-written-in-the-run)
  - [The join, in general](Domains#the-join-in-general)
  - [Precision limits](Domains#precision-limits-where-the-domains-stop-being-precise)
- [The Lean model](Formal-semantics)
  - [The four files](Formal-semantics#the-four-files)
  - [Lean and Rust](Formal-semantics#lean-and-rust)
  - [Primitives](Formal-semantics#primitives)
- [Primitive coverage](Primitive-coverage)
