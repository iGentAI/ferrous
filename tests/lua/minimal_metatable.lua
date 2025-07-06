-- Minimal metatable function test
local t = {}
local mt = {}
mt.__index = {value = 42}

-- Test setmetatable
setmetatable(t, mt)
print("setmetatable() called")

-- Test getmetatable
local mt2 = getmetatable(t)
print("getmetatable() returned: " .. tostring(mt == mt2))

-- Test __index metamethod
print("t.value (via __index): " .. t.value)

return "Metatable functions test successful"