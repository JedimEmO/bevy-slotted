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

-- A failed expectation is raised as a table rather than a string: `error`
-- prefixes a string with the chunk and line, which differs between the two
-- adapters and would make both the conformance cases and a mod's test report
-- depend on which runtime ran it. A table carries the message verbatim, and a
-- real Lua error (a typo in the test) still arrives with its position.

local T = {}
local tests = {}          -- array of { name = ..., fn = ... }
local running = nil       -- { name = ..., co = coroutine }

--- @group Tests
--- Register a test. The body runs as a coroutine: every action below yields to
--- the harness and resumes with its answer, so the test reads as straight-line
--- code while the app advances frames between the lines.
---
--- @signature slotted.test.test(name, fn)
--- @stage test
--- @param name string The test name, as it appears in the report.
--- @param fn function The body, called with no arguments.
--- @return nil
--- @luau test: (name: string, fn: () -> ()) -> ()
--- @example
--- slotted.test.test("the sort button sorts the player inventory", function()
---     slotted.test.open_screen("machine:furnace", "machine")
---     slotted.test.click({ test_id = "sorter_sort" })
---     slotted.test.settle()
---     slotted.test.expect_stack({ role = "slot", tag = { region = "player" }, index = 0 }, "demo:coal", 8)
--- end)
function T.test(name, fn)
    if type(name) ~= "string" or type(fn) ~= "function" then
        error("slotted.test.test(name, fn): name must be a string and fn a function", 2)
    end
    tests[#tests + 1] = { name = name, fn = fn }
end

-- Raise `message` as an expectation failure, with no position prefix.
local function fail(message)
    error({ __slotted_test = tostring(message) }, 0)
end

-- Yield one op to the host and return its answer, or raise its error.
local function step(op)
    local reply = coroutine.yield(op)
    if type(reply) ~= "table" then
        fail("the host resumed a test without a reply table")
    end
    if reply.error ~= nil then
        fail(reply.error)
    end
    return reply.value
end

-- Actions.

--- @group Actions
--- Open a screen over a fixture's inventories. Every test starts here.
---
--- @signature slotted.test.open_screen(kind, fixture)
--- @stage test
--- @param kind string The screen kind, `"mymod:chest"`.
--- @param fixture any `"empty"` (the default), the name of a fixture the host supplies, or an inline table: `slots` is the array of inventory sizes and `fill` maps `"<inventory>:<slot>"` to `{ item = ..., count = ... }`.
--- @return table `{ menu = <id>, screen = <kind> }`.
--- @luau open_screen: (kind: string, fixture: (string | { [string]: any })?) -> { [string]: any }
--- @example
--- local opened = slotted.test.open_screen("mymod:chest", {
---     slots = { 27, 27, 9 },
---     fill = { ["0:0"] = { item = "demo:cobblestone", count = 12 } },
--- })

--- Left-click a node.
---
--- A locator is a table with any of these keys, all of which must match:
--- `role` is a semantic role in snake case (`"slot"`, `"side_tab"`,
--- `"hud_layer"`), `tag` is a map of tags, `test_id` is the `test_id` tag,
--- `text` is the displayed text, `widget` is a `namespace:path` widget kind,
--- `item` matches a slot holding that item, and `index` picks the nth match in
--- tree order.
---
--- @signature slotted.test.click(loc)
--- @stage test
--- @param loc table A locator.
--- @return nil
--- @luau click: (loc: { [string]: any }) -> ()
--- @example
--- slotted.test.click({ role = "slot", tag = { region = "chest" }, index = 0 })

--- Shift-click a node, which is the quick-move mode.
---
--- @signature slotted.test.shift_click(loc)
--- @stage test
--- @param loc table A locator.
--- @return nil
--- @luau shift_click: (loc: { [string]: any }) -> ()
--- @example
--- slotted.test.shift_click({ test_id = "input" })

--- Right-click a node: pick up half, or place one.
---
--- @signature slotted.test.right_click(loc)
--- @stage test
--- @param loc table A locator.
--- @return nil
--- @luau right_click: (loc: { [string]: any }) -> ()
--- @example
--- slotted.test.right_click({ role = "slot", index = 0 })

--- Move the pointer over a node and leave it there, which is what raises a
--- tooltip.
---
--- @signature slotted.test.hover(loc)
--- @stage test
--- @param loc table A locator.
--- @return nil
--- @luau hover: (loc: { [string]: any }) -> ()
--- @example
--- slotted.test.hover({ test_id = "tank" })

--- Press and release one key.
---
--- @signature slotted.test.key(key)
--- @stage test
--- @param key string A Bevy `KeyCode` name: `"Escape"`, `"Tab"`, `"Digit1"`, `"KeyE"`, `"Enter"`.
--- @return nil
--- @luau key: (key: string) -> ()
--- @example
--- slotted.test.key("Escape")

--- Type text into the focused node.
---
--- @signature slotted.test.type_text(text)
--- @stage test
--- @param text string What to type.
--- @return nil
--- @luau type_text: (text: string) -> ()
--- @example
--- slotted.test.type_text("@mymod")

--- Advance virtual time until layout and motion are quiet. Call it after any
--- action whose effect takes a frame to appear.
---
--- @signature slotted.test.settle()
--- @stage test
--- @return nil
--- @luau settle: () -> ()
--- @example
--- slotted.test.settle()

--- Advance exactly `frames` frames, for the cases where settling would hide a
--- transient state.
---
--- @signature slotted.test.step(frames)
--- @stage test
--- @param frames number How many frames. Defaults to `1`.
--- @return nil
--- @luau step: (frames: number?) -> ()
--- @example
--- slotted.test.step(3)

--- Advance an icon button to its next state, or its previous one.
---
--- @signature slotted.test.cycle(loc, forward)
--- @stage test
--- @param loc table A locator for the icon button.
--- @param forward boolean Backwards when `false`. Defaults to `true`.
--- @return nil
--- @luau cycle: (loc: { [string]: any }, forward: boolean?) -> ()
--- @example
--- slotted.test.cycle({ test_id = "redstone_mode" })
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

--- @group Queries
--- What is in a slot.
---
--- @signature slotted.test.stack_at(loc)
--- @stage test
--- @param loc table A locator for the slot.
--- @return table `{ item = "ns:path", count = n }`, or `nil` when the slot is empty.
--- @luau stack_at: (loc: { [string]: any }) -> { item: string, count: number }?
--- @example
--- local s = slotted.test.stack_at({ test_id = "output" })

--- The rendered text of a node, after localisation.
---
--- @signature slotted.test.text_of(loc)
--- @stage test
--- @param loc table A locator.
--- @return string The text, or `nil` when the node has none.
--- @luau text_of: (loc: { [string]: any }) -> string?
--- @example
--- slotted.test.expect_eq(slotted.test.text_of({ test_id = "title" }), "Copper Chest")

--- The current value of the menu property a node is bound to.
---
--- @signature slotted.test.property_of(loc)
--- @stage test
--- @param loc table A locator for a bar, tank, progress arrow or icon button.
--- @return table `{ id = <property id>, value = <number> }`, or nothing when the node binds no property.
--- @luau property_of: (loc: { [string]: any }) -> { id: number, value: number }?
--- @example
--- slotted.test.expect_eq(slotted.test.property_of({ test_id = "redstone_mode" }).value, 1)

--- How full a tank is, from `0.0` to `1.0`.
---
--- @signature slotted.test.tank_fill(loc)
--- @stage test
--- @param loc table A locator for the tank.
--- @return number The fill fraction.
--- @luau tank_fill: (loc: { [string]: any }) -> number?
--- @example
--- slotted.test.expect(slotted.test.tank_fill({ test_id = "tank" }) > 0.5, "half full")

--- Whether a node exists and is visible.
---
--- @signature slotted.test.is_visible(loc)
--- @stage test
--- @param loc table A locator.
--- @return boolean
--- @luau is_visible: (loc: { [string]: any }) -> boolean
--- @example
--- slotted.test.expect(slotted.test.is_visible({ test_id = "tooltip" }), "a tooltip appeared")

--- Whether any console line so far contains `text`. The way to assert on what
--- a script logged.
---
--- @signature slotted.test.log_contains(text)
--- @stage test
--- @param text string A substring.
--- @return boolean
--- @luau log_contains: (text: string) -> boolean
--- @example
--- slotted.test.expect(slotted.test.log_contains("sorting"), "the handler ran")
function T.stack_at(loc) return step({ op = "stack_at", loc = loc }) end
function T.text_of(loc) return step({ op = "text_of", loc = loc }) end
function T.property_of(loc) return step({ op = "property_of", loc = loc }) end
function T.tank_fill(loc) return step({ op = "tank_fill", loc = loc }) end
function T.is_visible(loc) return step({ op = "is_visible", loc = loc }) end
function T.log_contains(text) return step({ op = "log_contains", text = text }) end

-- Expectations: pure Lua, no yield.

--- @group Expectations
--- Fail the test unless `cond` is truthy. Expectations are plain Lua and do
--- not yield.
---
--- @signature slotted.test.expect(cond, msg)
--- @stage test
--- @param cond any Anything; `false` and `nil` fail.
--- @param msg string The failure message. Defaults to `"expectation failed"`.
--- @return nil
--- @luau expect: (cond: any, msg: string?) -> ()
--- @example
--- slotted.test.expect(count == 3, "three stacks moved")
function T.expect(cond, msg)
    if not cond then
        fail(msg or "expectation failed")
    end
end


--- Fail the test unless `a == b`, reporting both values.
---
--- @signature slotted.test.expect_eq(a, b, msg)
--- @stage test
--- @param a any What you got.
--- @param b any What you expected.
--- @param msg string A prefix for the failure message.
--- @return nil
--- @luau expect_eq: (a: any, b: any, msg: string?) -> ()
--- @example
--- slotted.test.expect_eq(slotted.test.property_of({ test_id = "cook" }).value, 0, "cook progress")
function T.expect_eq(a, b, msg)
    if a ~= b then
        fail((msg and (msg .. ": ") or "") .. "expected " .. tostring(b) .. ", got " .. tostring(a))
    end
end


--- Fail the test unless a slot holds exactly this. Pass `nil` for `item` to
--- assert the slot is empty.
---
--- @signature slotted.test.expect_stack(loc, item, count)
--- @stage test
--- @param loc table A locator for the slot.
--- @param item string The expected item id, or `nil` for an empty slot.
--- @param count number The expected count. Any count matches when omitted.
--- @return nil
--- @luau expect_stack: (loc: { [string]: any }, item: string?, count: number?) -> ()
--- @example
--- slotted.test.expect_stack({ test_id = "output" }, "demo:iron_ingot", 1)
--- slotted.test.expect_stack({ test_id = "input" }, nil)
function T.expect_stack(loc, item, count)
    local s = T.stack_at(loc)
    if item == nil then
        if s ~= nil then
            fail("expected an empty slot, found " .. tostring(s.item) .. " x" .. tostring(s.count))
        end
        return
    end
    if s == nil then
        fail("expected " .. item .. " x" .. tostring(count) .. ", slot is empty")
    end
    if s.item ~= item or (count ~= nil and s.count ~= count) then
        fail("expected " .. item .. " x" .. tostring(count) .. ", found " .. s.item .. " x" .. tostring(s.count))
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

-- The message of a raised value: an expectation's own text, or whatever the
-- runtime made of a real error.
local function message_of(raised)
    if type(raised) == "table" and raised.__slotted_test ~= nil then
        return raised.__slotted_test
    end
    return tostring(raised)
end

local function finish(name, ok, result)
    running = nil
    if ok then
        return { type = "test_done", name = name, passed = true }
    end
    return { type = "test_done", name = name, passed = false, message = message_of(result) }
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
