-- Table Metamethods Test
-- Tests the complete set of table metamethods
-- Status: PARTIAL - Some metamethods are implemented, others are not

-- Create test tables
local t1 = {a = 1, b = 2}
local t2 = {x = 10, y = 20}

-- Create metatables
local mt1 = {}
local mt2 = {}

-- Set metatables
setmetatable(t1, mt1)
setmetatable(t2, mt2)

local results = {}

-- Test __index metamethod (reading undefined properties)
mt1.__index = {c = 3, d = 4}
results.index = t1.c == 3 and t1.d == 4
print("__index test:", results.index)

-- Test __newindex metamethod (writing to undefined properties)
local log = {}
mt2.__newindex = function(table, key, value)
  log[#log+1] = key .. "=" .. value
  rawset(table, key, value)
end

t2.z = 30  -- Should trigger __newindex
print("__newindex log:", table.concat(log, ", "))
results.newindex = log[1] == "z=30"

-- Test __call metamethod (calling a table)
mt1.__call = function(table, ...)
  local args = {...}
  return table.a + table.b + #args
end

local call_result = t1(10, 20, 30)
print("__call result:", call_result)
results.call = call_result == 6  -- 1 + 2 + 3(args)

-- Test arithmetic metamethods
mt1.__add = function(a, b) 
  return {a = a.a + b.x, b = a.b + b.y} 
end

local add_result = t1 + t2
print("__add result:", add_result.a, add_result.b)
results.add = add_result.a == 11 and add_result.b == 22

-- Test comparison metamethods
mt1.__eq = function(a, b)
  return a.a == b.x / 10 and a.b == b.y / 10
end

local eq_result = (t1 == t2)
print("__eq result:", eq_result)
results.eq = eq_result == true

-- Test __tostring metamethod
mt1.__tostring = function(t)
  return "Table{a=" .. t.a .. ", b=" .. t.b .. "}"
end

local tostring_result = tostring(t1)
print("__tostring result:", tostring_result)
results.tostring = tostring_result == "Table{a=1, b=2}"

-- Test __len metamethod
mt2.__len = function(t)
  return 100  -- Custom length implementation
end

local len_result = #t2
print("__len result:", len_result)
results.len = len_result == 100

-- Test __metatable protection
mt2.__metatable = "Protected"
print("getmetatable(t2):", getmetatable(t2))
results.metatable = getmetatable(t2) == "Protected"

-- Print overall results
local all_passed = true
for name, result in pairs(results) do
  if not result then
    all_passed = false
    print("FAILED:", name)
  end
end

print("All metamethod tests passed:", all_passed)

return all_passed