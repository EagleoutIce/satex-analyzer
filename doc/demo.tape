# Render: SATEX_LSP_PLUGIN=$HOME/.vim/pack/downloads/opt/lsp vhs doc/demo.tape (repo root, after cargo build --release), then shrink:
#   ffmpeg -i doc/demo.gif -vf "fps=20,scale=1000:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=48:stats_mode=diff[p];[b][p]paletteuse=dither=none:diff_mode=rectangle" /tmp/s.gif
#   gifsicle -O3 --lossy=150 --colors 35 /tmp/s.gif -o doc/demo.gif
# The Vim scene needs https://github.com/yegappan/lsp
Output doc/demo.gif

Require satex
Require vim

Set Shell "bash"
Set FontSize 24
Set Width 1700
Set Height 1000
Set Padding 16
Set TypingSpeed 31ms
Set Theme "Catppuccin Mocha"
Set WaitTimeout 600s

# Prewarm: the first run fills the package cache, so every shown run is warm
Hide
Type "export PATH=$PWD/target/release:$PATH && cd samples"
Enter
Type `satex() { local c=$1; shift; command satex $c --color always "$@"; }`
Enter
Type `for c in summary lint; do satex $c -f paper.tex >/dev/null 2>&1; done; satex query definitions -f paper.tex >/dev/null 2>&1; echo DO''NE`
Enter
Wait+Screen /DONE/
Type "clear"
Enter
Show

Type "# SaTeX supports various build systems, engines, and more!"
Enter
Type "satex --version"
Enter
Sleep 5s

# Type "clear"
# Enter
# Type "# Provide general document overviews "
# Enter
# Type "satex summary -f paper.tex --stats | head -20"
# Enter
# Sleep 4.5s

Type "clear"
Enter
Type "# Expansion-Sensitive Linting"
Enter
Type "satex lint -f paper.tex"
Enter
Sleep 5.5s

Type "clear"
Enter
Type "# Inspection of macros"
Enter
Type "satex explain -f paper.tex '\norm'"
Enter
Sleep 1.5s
Type "# These are also sensitive to the document position"
Enter
Type "satex explain -f paper.tex '\usepackage'"
Enter
Sleep 6s

# Type "# Inspect all parts affected by a toggle"
# Enter
# Type "satex slice -f paper.tex '\ifdraft' --forward | head -24"
# Enter
# Sleep 3.5s
Type "# Extract a figure/table/... with everything it requires to compile standalone"
Enter
Type "satex slice --at 45-48 -f paper.tex"
Enter
Sleep 5.5s

Type "# List all user-defined macros"
Enter
Type "satex query definitions -f paper.tex --filter 'origin=document' | head -12"
Enter
Sleep 3.5s
# Type "# List all (non-@) macros a package provides"
# Enter
# Type "satex query definitions -f paper.tex --filter 'package=mypackage and file~mypackage and not name~@'"
# Enter
# Sleep 5.5s

Type "clear"
Enter
Type "# Trace macro expansions interactively"
Enter
Type "satex trace -f paper.tex --lines 77:77 --interactive"
Enter
Sleep 1.5s
Enter
Sleep .75s
Enter
Sleep 2.5s

Type "clear"
Enter
Type "# Use SaTeX in your editor"
Enter
Sleep 1s
Type "vim -n -u ../doc/demo/vimrc paper.tex"
Enter
Sleep 2.5s
Type "/norm"
Enter
Sleep .25s
Type "K"
Sleep 2s
Type ":LspDiag show"
Enter
Sleep 2.5s
Type ":lclose"
Sleep .25s
Enter
Type "gg/WeirdMore"
Enter
Sleep 1s
Type ":LspRename"
Enter
Sleep 1s
Ctrl+U
Type "SuperWeird"
Sleep 1.5s
Enter
Sleep 3s
Type ":qa!"
Enter
Type "# Thank you for watching!"
Enter
Sleep 1s
Hide
Type "rm -f .*.sw? *~"
Enter
Show
