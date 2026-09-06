-- slotted_test.lua: the `slotted.test` module a mod's `tests/*.lua` sees.
--
-- Installed after slotted.lua, only for the "test" stage. A test body runs as
-- a coroutine: every harness action yields a `test_step` command to the host
-- and the host's `test_resume` event carries the answer back. Nothing here
-- touches the harness; it only builds tables. Contract:
-- docs/design/phase6-contract.md section 3.1.
--
-- Host protocol, through `__slotted_dispatch`:
--   { type = "test_list" }                 -> { { type = "test_list", names = {...} } }
--   { type = "test_run", name = n }        -> { <step or done> }
--   { type = "test_resume", value = v, error = e } -> { <step or done> }
-- where <step> is { type = "test_step", op = { op = "click", loc = {...} } }
-- and <done> is { type = "test_done", name = n, passed = bool, message = s }.

-- PHASE6-IMPL: C. The body below is the intended shape; fill the ops and the
-- dispatch hooks, and make the conformance suite cover them.

local T = {}
local tests = {}          -- array of { name = ..., fn = ... }
local running = nil       -- { name = ..., co = coroutine }

function T.test(name, fn)
    if type(name) ~= "string" or type(fn) ~= "function" then
        error("slotted.test.test(name, fn): name must be a string and fn a function", 2)
    end
    tests[#tests + 1] = { name = name, fn = fn }
end

-- Yield one op to the host and return its answer, or raise its error.
local function step(op)
    local reply = coroutine.yield(op)
    if reply.error ~= nil then
        error(reply.error, 2)
    end
    return reply.value
end

-- Actions.
function T.open_screen(kind, fixture) return step({ op = "open_screen", kind = kind, fixture = fixture or "empty" }) end
function T.click(loc) step({ op = "click", loc = loc }) end
function T.shift_click(loc) step({ op = "shift_click", loc = loc }) end
function T.right_click(loc) step({ op = "right_click", loc = loc }) end
function T.hover(loc) step({ op = "hover", loc = loc }) end
function T.key(key) step({ op = "key", key = key }) end
function T.type_text(text) step({ op = "type_text", text = text }) end
function T.settle() step({ op = "settle" }) end
function T.step(frames) step({ op = "step", frames = frames or 1 }) end
function T.cycle(loc, forward) step({ op = "cycle", loc = loc, forward = forward ~= false }) end

-- Queries.
function T.stack_at(loc) return step({ op = "stack_at", loc = loc }) end
function T.text_of(loc) return step({ op = "text_of", loc = loc }) end
function T.property_of(loc) return step({ op = "property_of", loc = loc }) end
function T.tank_fill(loc) return step({ op = "tank_fill", loc = loc }) end
function T.is_visible(loc) return step({ op = "is_visible", loc = loc }) end
function T.log_contains(text) return step({ op = "log_contains", text = text }) end

-- Expectations: pure Lua, no yield.
function T.expect(cond, msg)
    if not cond then
        error(msg or "expectation failed", 2)
    end
end

function T.expect_eq(a, b, msg)
    if a ~= b then
        error((msg and (msg .. ": ") or "") .. "expected " .. tostring(b) .. ", got " .. tostring(a), 2)
    end
end

function T.expect_stack(loc, item, count)
    local s = T.stack_at(loc)
    if item == nil then
        if s ~= nil then
            error("expected an empty slot, found " .. tostring(s.item) .. " x" .. tostring(s.count), 2)
        end
        return
    end
    if s == nil then
        error("expected " .. item .. " x" .. tostring(count) .. ", slot is empty", 2)
    end
    if s.item ~= item or (count ~= nil and s.count ~= count) then
        error("expected " .. item .. " x" .. tostring(count) .. ", found " .. s.item .. " x" .. tostring(s.count), 2)
    end
end

-- Dispatch hooks: what the host's three test events map to.
local function names()
    local out = {}
    for i, t in ipairs(tests) do
        out[i] = t.name
    end
    return out
end

local function finish(name, ok, result)
    running = nil
    if ok then
        return { type = "test_done", name = name, passed = true }
    end
    return { type = "test_done", name = name, passed = false, message = tostring(result) }
end

-- Resume the running coroutine with `reply`; return the next step or done.
local function continue(reply)
    local ok, result = coroutine.resume(running.co, reply)
    if not ok then
        return finish(running.name, false, result)
    end
    if coroutine.status(running.co) == "dead" then
        return finish(running.name, true)
    end
    return { type = "test_step", op = result }
end

function T.__list()
    return { type = "test_list", names = names() }
end

function T.__run(name)
    for _, t in ipairs(tests) do
        if t.name == name then
            running = { name = name, co = coroutine.create(t.fn) }
            return continue(nil)
        end
    end
    return finish(name, false, "no test named " .. tostring(name))
end

function T.__resume(value, err)
    if running == nil then
        return finish("?", false, "test_resume with no running test")
    end
    return continue({ value = value, error = err })
end

rawset(slotted, "test", T)
