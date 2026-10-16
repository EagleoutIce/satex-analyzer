$pdf_mode = 1;
$pdflatex = 'pdflatex -interaction=nonstopmode %O %S';
# pages 1 stacked, 2 wide, 3 wordmark
$success_cmd = 'for p in 1:satex 2:satex-wide 3:satex-wordmark; do pdftocairo -svg -f ${p%%:*} -l ${p%%:*} %D ../wiki/img/${p#*:}.svg; done';
