wget -O ./xlistings.sty https://raw.githubusercontent.com/EagleoutIce/xlistings/refs/heads/main/xlistings.sty
mkdir -p langs

LANGUAGES=("aspectj" "bash" "cpp" "csharp" "git" "haskell" "java" "javascript" "json" "lambda_calculus" "latex" "lua" "markdown" "mips" "python" "regex" "sql" "ts" "xml" "R")

for lang in "${LANGUAGES[@]}"; do
   wget -O "./langs/$lang.cfg" "https://raw.githubusercontent.com/EagleoutIce/xlistings/refs/heads/main/langs/$lang.cfg"
done