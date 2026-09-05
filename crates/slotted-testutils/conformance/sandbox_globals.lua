-- STAGE: data
-- EXPECT: [
--   (type: "log", level: "info", message: "all absent"),
-- ]
-- Every forbidden global is nil, and the kept ones are still there.
local forbidden = {
    "io", "os", "package", "require", "dofile", "loadfile", "load",
    "loadstring", "debug", "collectgarbage", "getfenv", "setfenv", "newproxy",
}
for _, name in ipairs(forbidden) do
    if _G[name] ~= nil then
        error("global " .. name .. " is still reachable")
    end
end
local kept = { "table", "string", "math", "bit32", "utf8", "pairs", "ipairs",
    "next", "select", "type", "tostring", "tonumber", "pcall", "xpcall",
    "error", "assert", "setmetatable", "getmetatable", "print" }
for _, name in ipairs(kept) do
    if _G[name] == nil then
        error("global " .. name .. " should be available")
    end
end
slotted.info("all absent")
