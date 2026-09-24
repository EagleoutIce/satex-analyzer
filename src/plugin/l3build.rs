//! l3build: the LaTeX Project's build system, configured by a `build.lua`
//! beside the sources (l3build manual, `texdoc l3build`, "The build.lua
//! file").  The file is Lua, so satex runs it: `texlua` evaluates it with
//! l3build's own `l3build-variables.lua` for the defaults, in a sandbox that
//! cannot write, run programs or load code.  Without `texlua` the plain
//! assignments are read instead.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::Engine;

pub const FILE: &str = "build.lua";

/// A variable of the configuration: l3build's are strings or lists of them.
#[derive(Clone, Debug, PartialEq)]
pub enum Var {
    Str(String),
    List(Vec<String>),
}

/// What a `build.lua` configures, and how satex learned it.
#[derive(Clone, Debug, Default)]
pub struct L3build {
    pub path: PathBuf,
    pub root: PathBuf,
    /// `texlua`, `texlua, then read` when the run stopped early, or `read`.
    pub how: &'static str,
    pub vars: BTreeMap<String, Var>,
    /// Why `texlua` stopped, when it did.
    pub error: Option<String>,
}

/// The variables a run reports (l3build manual, "Variables"): which module,
/// where its files are, which engines check and typeset it.
const REPORTED: [&str; 7] =
    ["module", "sourcefiledir", "supportdir", "testfiledir", "checkengines", "stdengine", "typesetexe"];

/// Runs `build.lua` in a table whose unknown globals are a stub that absorbs
/// every index, call and concatenation, so a configuration written against
/// l3build's functions (`options`, `uploadconfig`, `target_list`) still
/// assigns its variables.  Then `l3build-variables.lua` fills in the rest,
/// as `l3build.lua` does after `dofile("build.lua")`.  Every string or
/// list-of-strings global is printed; `=` marks one `build.lua` set.
const EVALUATOR: &str = r#"
local build, defaults = arg[1], arg[2]
local stub = setmetatable({}, {})
local mt = getmetatable(stub)
mt.__index = function(_, k) if type(k) == "string" then return stub end end
mt.__call = function() return stub end
mt.__concat = function(a, b) if a == stub then return b == stub and "" or b end return a end
mt.__len = function() return 0 end
mt.__tostring = function() return "" end
mt.__eq = function() return false end
local function readonly(name, mode)
  if mode and not mode:match("^r") then return nil, "read-only" end
  return io.open(name, mode)
end
local handle = { read = function() return "" end, close = function() return true end,
  lines = function() return function() return nil end end }
local safe = {
  string = string, table = table, math = math, pairs = pairs, ipairs = ipairs,
  type = type, tostring = tostring, tonumber = tonumber, select = select,
  next = next, pcall = pcall, error = error, assert = assert, unpack = table.unpack,
  setmetatable = setmetatable, getmetatable = getmetatable, rawget = rawget,
  rawset = rawset, print = function() end, status = status,
  os = { getenv = os.getenv, date = os.date, time = os.time, clock = os.clock,
    type = os.type, execute = function() return 0 end, exit = function() error("exit") end },
  io = { open = readonly, popen = function() return handle end, read = function() return "" end,
    write = function() end, lines = function() return function() return nil end end },
  lfs = lfs and { attributes = lfs.attributes, dir = lfs.dir, currentdir = lfs.currentdir,
    isdir = lfs.isdir, isfile = lfs.isfile },
  require = function() return stub end, dofile = function() return stub end,
  loadfile = function() return nil end,
  fileexists = function(f) local h = io.open(f, "r") if h then h:close() return true end return false end,
  direxists = function(d) return lfs and lfs.isdir and lfs.isdir(d) or false end,
}
local env = setmetatable({}, { __index = function(_, k)
  local v = safe[k] if v ~= nil then return v end
  if type(k) == "string" then return stub end end })
local chunk, err = loadfile(build, "t", env)
local ok = chunk ~= nil
if chunk then ok, err = pcall(chunk) end
if not ok then io.write("!\t", tostring(err):gsub("[\t\n]", " "), "\n") end
local set = {}
for k, v in pairs(env) do if v ~= stub then set[k] = true else env[k] = nil end end
for k, v in pairs(env) do if type(v) == "table" then for i, x in ipairs(v) do if x == stub then v[i] = nil end end end end
getmetatable(env).__index = safe
if defaults and defaults ~= "" then
  local d = loadfile(defaults, "t", env)
  if d then pcall(d) end
end
local function strings(v)
  if type(v) ~= "table" then return nil end
  local out = {}
  for _, x in ipairs(v) do if type(x) ~= "string" then return nil end out[#out + 1] = x end
  local n = 0 for _ in pairs(v) do n = n + 1 end
  if n ~= #out then return nil end
  return out
end
for k, v in pairs(env) do
  if type(k) == "string" then
    local mark = set[k] and "=" or ""
    if type(v) == "string" then io.write(mark, k, "\ts\t", (v:gsub("[\t\n]", " ")), "\n")
    else
      local list = strings(v)
      if list then io.write(mark, k, "\tl\t", table.concat(list, "\31"), "\n") end
    end
  end
end
"#;

impl L3build {
    /// The configuration of the bundle `dir` belongs to: the `build.lua` in
    /// it or in the nearest directory above, within the repository, since
    /// l3build runs from the bundle's main directory while its tests and
    /// documentation sit below it.
    pub fn read(dir: &Path) -> Option<L3build> {
        let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        let mut root = None;
        for ancestor in dir.ancestors() {
            if ancestor.join(FILE).is_file() {
                root = Some(ancestor);
                break;
            }
            if ancestor.join(".git").exists() {
                break;
            }
        }
        let root = root?;
        let path = root.join(FILE);
        let text = std::fs::read_to_string(&path).ok()?;
        let defaults = defaults_file();
        let mut config = L3build { path: path.clone(), root: root.to_path_buf(), ..L3build::default() };
        let written = assignments(&text, &BTreeMap::new());
        match evaluate(root, defaults.as_deref()) {
            Some((vars, error)) => {
                config.how = if error.is_some() { "texlua, then read" } else { "texlua" };
                config.vars = vars.iter().map(|(k, (v, _))| (k.clone(), v.clone())).collect();
                // What the run did not reach is read from the file itself.
                if error.is_some() {
                    for (key, value) in written {
                        if !vars.get(&key).is_some_and(|(_, set)| *set) {
                            config.vars.insert(key, value);
                        }
                    }
                }
                config.error = error;
            }
            None => {
                config.how = "read";
                let base = defaults
                    .and_then(|file| std::fs::read_to_string(file).ok())
                    .map(|text| assignments(&text, &written))
                    .unwrap_or_default();
                config.vars = base;
                config.vars.extend(written);
            }
        }
        Some(config)
    }

    pub fn string(&self, key: &str) -> Option<&str> {
        match self.vars.get(key)? {
            Var::Str(s) => Some(s),
            Var::List(_) => None,
        }
    }

    pub fn list(&self, key: &str) -> Vec<String> {
        match self.vars.get(key) {
            Some(Var::List(items)) => items.clone(),
            Some(Var::Str(s)) => vec![s.clone()],
            None => Vec::new(),
        }
    }

    /// A directory variable, relative to where `build.lua` is.
    pub fn dir(&self, key: &str) -> PathBuf {
        let value = self.string(key).unwrap_or(".");
        let joined = self.root.join(value);
        normalize(&joined)
    }

    /// Where the files under development are found: the source and support
    /// directories, and every directory a `sourcefiles` pattern reaches into
    /// (l3build copies them all flat into one directory before it runs TeX).
    pub fn search_paths(&self) -> Vec<PathBuf> {
        let source = self.dir("sourcefiledir");
        let mut out = vec![source.clone()];
        for key in ["supportdir", "testsuppdir"] {
            let dir = self.dir(key);
            if dir.is_dir() && !out.contains(&dir) {
                out.push(dir);
            }
        }
        for file in self.files("sourcefiledir", "sourcefiles") {
            if let Some(dir) = file.parent().map(Path::to_path_buf)
                && !out.contains(&dir)
            {
                out.push(dir);
            }
        }
        out
    }

    /// The files a pattern list selects inside a directory variable.
    pub fn files(&self, dir: &str, patterns: &str) -> Vec<PathBuf> {
        let base = self.dir(dir);
        let mut out = Vec::new();
        for pattern in self.list(patterns) {
            let joined = base.join(&pattern);
            let Some(joined) = joined.to_str() else { continue };
            for path in glob::glob(joined).into_iter().flatten().flatten() {
                if path.is_file() && !out.contains(&path) {
                    out.push(path);
                }
            }
        }
        out.sort();
        out
    }

    /// What unpacking makes: every file the `.ins` batch files in
    /// `unpackfiles` generate, with the `.dtx` and guards it comes from
    /// (docstrip.dtx, "The user interface").  A load of one of them reads the
    /// `.dtx` the way docstrip would extract it.
    pub fn unpacked(&self) -> BTreeMap<String, (PathBuf, Vec<String>)> {
        let mut out = BTreeMap::new();
        let source = self.dir("sourcefiledir");
        for ins in self.files("sourcefiledir", "unpackfiles") {
            let Ok(text) = std::fs::read_to_string(&ins) else { continue };
            for generated in crate::literate::generated(&text) {
                let from = source.join(&generated.from);
                let from = if from.is_file() {
                    from
                } else {
                    ins.with_file_name(&generated.from)
                };
                if !from.is_file() {
                    continue;
                }
                let guards = generated.guards.split(',').map(|g| g.trim().to_string()).collect();
                out.entry(generated.file).or_insert((from, guards));
            }
        }
        out
    }

    /// The documents l3build typesets: `typesetfiles`, looked for in the
    /// documentation and source directories.
    pub fn documents(&self) -> Vec<PathBuf> {
        let mut out = self.files("docfiledir", "typesetfiles");
        for path in self.files("sourcefiledir", "typesetfiles") {
            if !out.contains(&path) {
                out.push(path);
            }
        }
        out
    }

    /// The files it installs: what the package is to its users.
    pub fn library(&self) -> Vec<PathBuf> {
        self.files("sourcefiledir", "installfiles")
    }

    /// The regression tests: `.lvt` files in `testfiledir` (l3build manual,
    /// "Regression testing").
    pub fn tests(&self) -> Vec<PathBuf> {
        let dir = self.dir("testfiledir");
        let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|e| e == "lvt" || e == "pvt"))
            .collect();
        out.sort();
        out
    }

    /// The engines `l3build check` runs every test with.
    pub fn check_engines(&self) -> Vec<Engine> {
        self.list("checkengines").iter().filter_map(|name| Engine::from_program(name)).collect()
    }

    /// The engine a file is run with: `typesetexe` for a document l3build
    /// typesets, `stdengine` for a test or anything else it checks.
    pub fn engine_for(&self, file: Option<&Path>) -> Option<Engine> {
        let file = file.map(normalize);
        let typeset = file.as_ref().is_some_and(|file| self.documents().contains(file));
        let program = match typeset {
            true => self.string("typesetexe")?.to_string(),
            false => self
                .string("stdengine")
                .map(str::to_string)
                .or_else(|| self.list("checkengines").into_iter().next())?,
        };
        Engine::from_program(program.split_whitespace().next()?)
    }

    /// The settings shown for the build file.
    pub fn settings(&self) -> Vec<(String, String)> {
        let mut out = vec![("evaluated".to_string(), self.how.to_string())];
        for key in REPORTED {
            let value = match self.vars.get(key) {
                Some(Var::Str(s)) => s.clone(),
                Some(Var::List(items)) => items.join(" "),
                None => continue,
            };
            out.push((key.to_string(), value));
        }
        out
    }
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir if out.file_name().is_some() => {
                out.pop();
            }
            part => out.push(part),
        }
    }
    out
}

/// l3build's own defaults, where the installation keeps them.
fn defaults_file() -> Option<PathBuf> {
    let output =
        std::process::Command::new("kpsewhich").arg("l3build-variables.lua").output().ok()?;
    let path = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

type Evaluated = (BTreeMap<String, (Var, bool)>, Option<String>);

fn evaluate(root: &Path, defaults: Option<&Path>) -> Option<Evaluated> {
    let texlua = which::which("texlua").ok()?;
    let script = std::env::temp_dir()
        .join(format!("satex-l3build-{}-{:?}.lua", std::process::id(), std::thread::current().id()));
    std::fs::write(&script, EVALUATOR).ok()?;
    let defaults = defaults.map(|p| p.display().to_string()).unwrap_or_default();
    let output = std::process::Command::new(texlua)
        .arg(&script)
        .arg(FILE)
        .arg(defaults)
        .current_dir(root)
        .stdin(std::process::Stdio::null())
        .output();
    let _ = std::fs::remove_file(&script);
    let output = output.ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    let mut vars = BTreeMap::new();
    let mut error = None;
    for line in text.lines() {
        let mut parts = line.splitn(3, '\t');
        let (Some(key), Some(kind), Some(value)) = (parts.next(), parts.next(), parts.next()) else {
            if let Some(message) = line.strip_prefix("!\t") {
                error = Some(message.to_string());
            }
            continue;
        };
        if key == "!" {
            error = Some(format!("{kind}\t{value}"));
            continue;
        }
        let (key, set) = match key.strip_prefix('=') {
            Some(key) => (key, true),
            None => (key, false),
        };
        let value = match kind {
            "s" => Var::Str(value.to_string()),
            "l" if value.is_empty() => Var::List(Vec::new()),
            "l" => Var::List(value.split('\u{1f}').map(str::to_string).collect()),
            _ => continue,
        };
        vars.insert(key.to_string(), (value, set));
    }
    if let Some(message) = &mut error {
        *message = message.trim().to_string();
    }
    Some((vars, error))
}

/// The assignments that need no Lua to read: `x = "…"`, `x = {"a", "b"}`,
/// `x = x or …`, `..` between strings and variables already known, and
/// `table.unpack(t)` inside a table.  Anything else leaves the variable out.
pub fn assignments(text: &str, known: &BTreeMap<String, Var>) -> BTreeMap<String, Var> {
    let mut vars: BTreeMap<String, Var> = BTreeMap::new();
    let tokens = super::lua::lex(text);
    let mut at = 0;
    // Only statements at the top level of the chunk: what a function body
    // or a conditional assigns depends on how the file is run.
    let mut depth = 0usize;
    while at < tokens.len() {
        match tokens[at].as_str() {
            "function" | "do" | "then" | "repeat" => depth += 1,
            "end" | "until" | "elseif" => depth = depth.saturating_sub(1),
            _ => {}
        }
        let is_name = |t: &str| t.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_');
        let statement_start =
            at == 0 || !matches!(tokens[at - 1].as_str(), "." | ":" | "local" | "," | "(" | "=" | "..");
        if depth == 0
            && statement_start
            && is_name(&tokens[at])
            && tokens.get(at + 1).is_some_and(|t| t == "=")
        {
            let name = tokens[at].clone();
            let mut next = at + 2;
            let lookup = |key: &str, vars: &BTreeMap<String, Var>| {
                vars.get(key).or_else(|| known.get(key)).cloned()
            };
            match expression(&tokens, &mut next, &|key| lookup(key, &vars)) {
                Some(value) => {
                    vars.insert(name, value);
                }
                None => {
                    vars.remove(&name);
                }
            }
            at = next.max(at + 1);
            continue;
        }
        at += 1;
    }
    vars
}

fn expression(tokens: &[String], at: &mut usize, lookup: &dyn Fn(&str) -> Option<Var>) -> Option<Var> {
    let mut value = concatenation(tokens, at, lookup);
    while tokens.get(*at).is_some_and(|t| t == "or") {
        *at += 1;
        let other = concatenation(tokens, at, lookup);
        value = value.or(other);
    }
    value
}

fn concatenation(tokens: &[String], at: &mut usize, lookup: &dyn Fn(&str) -> Option<Var>) -> Option<Var> {
    let mut value = term(tokens, at, lookup);
    while tokens.get(*at).is_some_and(|t| t == "..") {
        *at += 1;
        let right = term(tokens, at, lookup);
        value = match (value, right) {
            (Some(Var::Str(a)), Some(Var::Str(b))) => Some(Var::Str(a + &b)),
            _ => None,
        };
    }
    value
}

fn term(tokens: &[String], at: &mut usize, lookup: &dyn Fn(&str) -> Option<Var>) -> Option<Var> {
    let token = tokens.get(*at)?.clone();
    *at += 1;
    if let Some(text) = token.strip_prefix('"') {
        return Some(Var::Str(text.to_string()));
    }
    if token == "{" {
        let mut items = Vec::new();
        let mut known = true;
        while let Some(t) = tokens.get(*at) {
            if t == "}" {
                *at += 1;
                break;
            }
            if t == "," || t == ";" {
                *at += 1;
                continue;
            }
            if t == "table" && tokens.get(*at + 1).is_some_and(|t| t == ".") {
                *at += 3;
                let inner = tokens.get(*at + 1).cloned().unwrap_or_default();
                *at += 3;
                match lookup(&inner) {
                    Some(Var::List(list)) => items.extend(list),
                    _ => known = false,
                }
                continue;
            }
            match expression(tokens, at, lookup) {
                Some(Var::Str(s)) => items.push(s),
                _ => {
                    known = false;
                    // Skip to the next item.
                    let mut depth = 0usize;
                    while let Some(t) = tokens.get(*at) {
                        match t.as_str() {
                            "{" | "(" => depth += 1,
                            "}" | ")" if depth == 0 => break,
                            "}" | ")" => depth -= 1,
                            "," if depth == 0 => break,
                            _ => {}
                        }
                        *at += 1;
                    }
                }
            }
        }
        return known.then_some(Var::List(items));
    }
    if token.chars().next().is_some_and(|c| c.is_alphabetic() || c == '_') {
        // A call or a field is something only running the file decides.
        if tokens.get(*at).is_some_and(|t| matches!(t.as_str(), "(" | "." | ":" | "[")) {
            return None;
        }
        return lookup(&token);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_assignments() {
        let vars = assignments(
            "module = \"demo\" -- a comment\nsourcefiles = {\"a.dtx\", 'b.ins'}\nsupportdir = module .. \"/support\"\nlocal x = 1\nfunction f() typesetexe = \"lualatex\" end\n",
            &BTreeMap::new(),
        );
        assert_eq!(vars.get("module"), Some(&Var::Str("demo".into())));
        assert_eq!(vars.get("sourcefiles"), Some(&Var::List(vec!["a.dtx".into(), "b.ins".into()])));
        assert_eq!(vars.get("supportdir"), Some(&Var::Str("demo/support".into())));
        assert_eq!(vars.get("typesetexe"), None);
        assert_eq!(vars.get("x"), None);
    }

    #[test]
    fn reads_defaults_the_way_l3build_writes_them() {
        let known = assignments("maindir = \"src\"\n", &BTreeMap::new());
        let vars = assignments("maindir = maindir or \".\"\nsupportdir = supportdir or maindir .. \"/support\"\ncheckengines = checkengines or {\"pdftex\", \"xetex\"}\n", &known);
        assert_eq!(vars.get("maindir"), Some(&Var::Str("src".into())));
        assert_eq!(vars.get("supportdir"), Some(&Var::Str("src/support".into())));
        assert_eq!(vars.get("checkengines"), Some(&Var::List(vec!["pdftex".into(), "xetex".into()])));
    }

    #[test]
    fn unpacks_table_unpack() {
        let vars = assignments("s = {\"*.lua\"}\nsourcefiles = {\"*.dtx\", table.unpack(s)}\n", &BTreeMap::new());
        assert_eq!(vars.get("sourcefiles"), Some(&Var::List(vec!["*.dtx".into(), "*.lua".into()])));
    }
}
