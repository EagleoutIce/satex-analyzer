-- The LuaTeX globals over the libraries satex implements (lua_run.rs).
-- A function LuaTeX has and satex does not implement fails the run when
-- it is called; a value satex does not know is the unknown value `marker`,
-- which the patched VM does not let a test, comparison, key, conversion or
-- `type` see; a field LuaTeX does not have is nil.
local libs = ...
local satex = libs.satex
local marker, unknown, failed, tainted = satex.marker, satex.unknown, satex.failed, satex.tainted
local find = string.find
local rawpcall, rawxpcall, rawload, error, type, select = pcall, xpcall, load, error, type, select
local setmetatable, ipairs, pairs = setmetatable, ipairs, pairs
local pack, unpack = table.pack, table.unpack

-- An unknown function: calling it fails the run.
local function unknown_function() unknown() end

-- What LuaTeX has (luatex_fields.txt): `globals[name]` and
-- `fields[library][name]` are the type of the value.
local globals, fields = {}, {}
for line in satex.fields:gmatch("[^\n]+") do
  local kind, a, b, c = line:match("^([GF]) (%S+) (%S+) ?(%S*)$")
  if kind == "G" then globals[a] = b
  elseif kind == "F" then fields[a] = fields[a] or {} fields[a][b] = c end
end

-- The value LuaTeX has and satex does not: a function that fails the run
-- when called, anything else the unknown value.
local function lacking(ty)
  if ty == "function" then return unknown_function end
  if ty ~= nil then return marker end
end

-- `t` with the fields LuaTeX's `library` has and `t` lacks; without a
-- library, every field it lacks is a function satex does not implement.
local function partly(t, library)
  local known = fields[library]
  return setmetatable(t or {}, { __index = function(_, k)
    if not known then return unknown_function end
    return lacking(known[k])
  end })
end

-- An unknown is not an error Lua may catch.
local function is_unknown(e)
  return failed() or (rawequal(type(e), "string") and find(e, "satex: unknown", 1, true))
end
local function check(ok, ...)
  if not ok and is_unknown((...)) then error((...), 0) end
  return ok, ...
end
pcall = function(f, ...) return check(rawpcall(f, ...)) end
xpcall = function(f, handler, ...)
  return check(rawxpcall(f, function(e)
    if is_unknown(e) then return e end
    return handler(e)
  end, ...))
end
local resume = coroutine.resume
coroutine.resume = function(co, ...) return check(resume(co, ...)) end
-- LuaTeX's Lua keeps these from 5.1 and 5.2.
_G.unpack, _G.loadstring = table.unpack, function(s, name) return rawload(s, name, "t") end
math.pow = function(x, y) return x ^ y end
math.log10 = function(x) return math.log(x, 10) end
math.ldexp = function(m, e) return m * 2.0 ^ e end
-- No precompiled chunks.
load = function(chunk, name, _, ...) return rawload(chunk, name, "t", ...) end
-- The terminal is not the document.
print = function() end
collectgarbage = function(opt) if opt == "count" then return marker end return 0 end

local function within(file, f, ...)
  satex.enter_file(file)
  local r = pack(rawpcall(f, ...))
  satex.leave_file(r[1])
  if not r[1] then error(r[2], 0) end
  return unpack(r, 2, r.n)
end
loadfile = function(name) return (satex.loadfile(name)) end
dofile = function(name)
  local f, file = satex.loadfile(name)
  return within(file, f)
end

-- Lua manual § 6.3, with satex's resolver as the file searcher.
package = { loaded = {}, preload = {}, path = "", cpath = "", config = "/\n;\n?\n!\n-\n",
  searchpath = function() unknown() end, loadlib = function() unknown() end }
package.searchers = {
  function(name)
    local f = package.preload[name]
    if f then return f, ":preload:" end
    return "\n\tno field package.preload['" .. name .. "']"
  end,
  function(name)
    local f, path, file = satex.searcher(name)
    return function(...) return within(file, f, ...) end, path
  end,
}
function require(name)
  local loaded = package.loaded
  if loaded[name] ~= nil then return loaded[name] end
  for _, searcher in ipairs(package.searchers) do
    local loader, data = searcher(name)
    if type(loader) == "function" then
      local r = loader(name, data)
      if r ~= nil then loaded[name] = r elseif loaded[name] == nil then loaded[name] = true end
      return loaded[name]
    end
  end
  error("module '" .. name .. "' not found", 2)
end

-- LuaTeX manual, "The token library".
local tok = libs.token
tok.is_token = satex.is_token
tok.type = function(t) if satex.is_token(t) then return "token" end end
local commands = satex.commands
tok.command_id = function(name) return commands[name] end
tok.biggest_char = function() return 0x10FFFF end
for _, f in ipairs { "command", "cmdname", "csname", "active", "expandable", "protected", "mode", "index", "id", "tok" } do
  local name = f == "command" and "get_command" or "get_" .. f
  tok[name] = function(t) return t[f] end
end
token = partly(tok, "token")

-- LuaTeX manual, "The tex library".
local function registers(kind)
  return setmetatable({}, {
    __index = function(_, k) return satex.register_get(kind, k) end,
    __newindex = function(_, k, v) satex.register_set(kind, false, k, v) end,
  })
end
local function setter(kind)
  return function(...)
    if select("#", ...) >= 3 then
      local prefix, k, v = ...
      return satex.register_set(kind, prefix == "global", k, v)
    end
    local k, v = ...
    return satex.register_set(kind, false, k, v)
  end
end
local t = libs.tex
t.count, t.dimen = registers("count"), registers("dimen")
t.getcount = function(k) return satex.register_get("count", k) end
t.getdimen = function(k) return satex.register_get("dimen", k) end
t.setcount, t.setdimen = setter("count"), setter("dimen")
t.hashtokens = function() return partly() end
t.runtoks = function(f)
  if type(f) ~= "function" then unknown() end
  satex.capture(true)
  local r = pack(rawpcall(f))
  satex.capture(false)
  if not r[1] then error(r[2], 0) end
end
tex = setmetatable(t, { __index = function(_, k) return satex.tex_index(k) end })

texio = partly({ write = function() end, write_nl = function() end }, "texio")
lua = partly({ get_functions_table = function() return satex.functions end,
  newtable = function() return {} end, bytecode = {}, name = {},
  version = "Lua 5.3" }, "lua")
kpse = partly({ find_file = libs.kpse.find_file }, "kpse")
lfs = partly({
  attributes = libs.lfs.attributes,
  isfile = function(p) return libs.lfs.attributes(p, "mode") == "file" end,
  isdir = function(p) return libs.lfs.attributes(p, "mode") == "directory" end,
}, "lfs")
unicode = { utf8 = partly({ char = utf8.char }), ascii = partly(), latin1 = partly(), grapheme = partly() }
-- Status values are unknown; its functions fail.
status = setmetatable({}, { __index = function(_, k)
  local f = libs.status[k]
  if f then return f() end
  return marker
end })

-- LuaTeX manual, "Callbacks": registering one whose effect satex does not
-- read is harmless; any other makes what follows unknown.
local harmless = {}
for _, n in ipairs {
  "pre_dump", "start_run", "stop_run", "wrapup_run", "start_page_number", "stop_page_number",
  "show_error_hook", "show_error_message", "show_lua_error_hook", "show_warning_message",
  "finish_pdffile", "finish_pdfpage", "start_file", "stop_file", "call_edit", "finish_synctex",
  "input_level_string", "page_order_index",
} do harmless[n] = true end
local registered, numbers, count = {}, {}, 0
callback = partly({
  register = function(name, f)
    if not harmless[name] then unknown() end
    registered[name] = f or nil
    if not numbers[name] then count = count + 1 numbers[name] = count end
    return numbers[name]
  end,
  find = function(name) return registered[name] end,
}, "callback")

lpeg = libs.lpeg

-- `io`, reading only files the resolver finds.
local function file_of(data)
  local at, closed = 1, false
  local f = {}
  local function one(fmt)
    if type(fmt) == "number" then
      if at > #data then return nil end
      local s = data:sub(at, at + fmt - 1)
      at = at + #s
      return s
    end
    fmt = fmt:gsub("^%*", "")
    local c = fmt:sub(1, 1)
    if c == "a" then
      local s = data:sub(at)
      at = #data + 1
      return s
    elseif c == "l" or c == "L" then
      if at > #data then return nil end
      local e = data:find("\n", at, true)
      local s = data:sub(at, e or #data)
      at = (e or #data) + 1
      if c == "l" then s = s:gsub("\n$", "") end
      return s
    end
    unknown()
  end
  function f:read(...)
    if closed then error("attempt to use a closed file") end
    local n = select("#", ...)
    if n == 0 then return one("l") end
    local r = {}
    for i = 1, n do
      r[i] = one((select(i, ...)))
      if r[i] == nil then return unpack(r, 1, i) end
    end
    return unpack(r, 1, n)
  end
  function f:lines(...)
    local fmts = pack(...)
    if fmts.n == 0 then fmts = { "l", n = 1 } end
    return function() return self:read(unpack(fmts, 1, fmts.n)) end
  end
  function f:close() closed = true return true end
  function f:seek(whence, offset)
    whence, offset = whence or "cur", offset or 0
    if whence == "set" then at = offset + 1
    elseif whence == "end" then at = #data + offset + 1
    elseif whence == "cur" then at = at + offset
    else unknown() end
    return at - 1
  end
  function f:write() unknown() end
  f.setvbuf, f.flush = unknown_function, unknown_function
  return f
end
io = partly({
  open = function(name, mode)
    mode = mode or "r"
    if mode ~= "r" and mode ~= "rb" then unknown() end
    local data, err = satex.read_file(name)
    if not data then return nil, err, 2 end
    return file_of(data)
  end,
  lines = function(name, ...)
    if name == nil then unknown() end
    local data, err = satex.read_file(name)
    if not data then error(err, 2) end
    return file_of(data):lines(...)
  end,
}, "io")

-- LuaTeX's string reading library (big-endian numbers at a position).
sio = partly({}, "sio")
for n = 1, 4 do
  sio["readinteger" .. n] = function(s, at) return (string.unpack(">i" .. n, s, at)) end
  sio["readcardinal" .. n] = function(s, at) return (string.unpack(">I" .. n, s, at)) end
end

-- LuaTeX manual, "The node library": node types and whatsit subtypes are
-- numbers; nodes themselves are unknown.
local node_types = {}
for i, n in ipairs {
  "hlist", "vlist", "rule", "ins", "mark", "adjust", "boundary", "disc", "whatsit", "local_par", "dir",
  "math", "glue", "kern", "penalty", "unset", "style", "choice", "noad", "radical", "fraction", "accent",
  "fence", "math_char", "sub_box", "sub_mlist", "math_text_char", "delim", "margin_kern", "glyph",
  "align_record", "pseudo_file", "pseudo_line", "page_insert", "split_insert", "expr_stack",
  "nested_list", "span", "attribute", "glue_spec", "attribute_list", "temp", "align_stack",
  "movement_stack", "if_stack", "unhyphenated", "hyphenated", "delta", "passive", "shape",
} do node_types[n] = i - 1 end
local whatsits = { open = 0, write = 1, close = 2, special = 3, save_pos = 6, late_lua = 7, user_defined = 8 }
node = partly({
  id = function(name) return node_types[name] or unknown() end,
  subtype = function(name) return whatsits[name] or unknown() end,
  direct = partly(),
}, "node")

-- The clock is unknown.
os = partly({ date = libs.os.date, gettimeofday = function() return marker end,
  clock = function() return marker end, time = function() return marker end }, "os")

-- Every other library LuaTeX has, with what satex lacks of it; LuaHBTeX's
-- HarfBuzz only when the engine is known to be LuaHBTeX (below).
local harfbuzz = { luaharfbuzz = true, luaharfbuzzsubset = true }
for name, ty in pairs(globals) do
  if rawget(_G, name) == nil and not harfbuzz[name] then
    if ty == "table" then _G[name] = partly({}, name) else _G[name] = lacking(ty) end
  end
end
for name in pairs(fields) do
  local t = rawget(_G, name)
  if type(t) == "table" and not getmetatable(t) and not harfbuzz[name] then partly(t, name) end
end
for name, ty in pairs(globals) do
  if ty == "table" and not harfbuzz[name] then package.loaded[name] = rawget(_G, name) end
end

-- A global no run satex finished may have set is unknown.
setmetatable(_G, { __index = function(_, k)
  if harfbuzz[k] then
    if status.luatex_engine == "luahbtex" then
      local t = partly({}, k)
      rawset(_G, k, t)
      package.loaded[k] = t
      return t
    end
    return nil
  end
  if tainted(k) then unknown() end
end })
