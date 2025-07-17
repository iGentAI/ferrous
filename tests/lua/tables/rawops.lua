-- Minimal raw table operations test
local t = {}
local mt = {
  __index = {value = 42},
  __newindex = function(table, key, value)
    rawset(table, key .. "_modified", value)
  end
}

-- Set up metatable
setmetatable(t, mt)
print("Metatable with __index and __newindex set")

-- Test rawset
rawset(t, "direct", "value")
print("rawset(t, \"direct\", \"value\") => " .. tostring(t.direct))

-- Normal assignment (triggers metamethod)
t.normal = "test"
print("t.normal = \"test\" => t.normal = " .. tostring(t.normal))
print("t.normal_modified = " .. tostring(t.normal_modified))

-- Test rawget
local normal_value = rawget(t, "normal")
print("rawget(t, \"normal\") => " .. tostring(normal_value))

-- Test rawequal
local a = {}
local b = {}
print("rawequal(a, a) => " .. tostring(rawequal(a, a)))
print("rawequal(a, b) => " .. tostring(rawequal(a, b)))
print("rawequal(1, 1) => " .. tostring(rawequal(1, 1)))

return "Raw table operations test successful"