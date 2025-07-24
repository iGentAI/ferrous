-- Complex Concatenation Test
-- This test validates the completed ConcatContinuation implementation
-- including multi-value concatenation and metamethod handling

-- Test 1: Multi-value string concatenation
local str1 = "Hello"
local str2 = " "
local str3 = "Beautiful"
local str4 = " "
local str5 = "World"

-- Complex concatenation that triggers ConcatContinuation logic
local result1 = str1 .. str2 .. str3 .. str4 .. str5
print("Multi-string concat:", result1)
assert(result1 == "Hello Beautiful World", "Multi-string concatenation failed")

-- Test 2: Mixed type concatenation (strings and numbers)
local mixed_result = "Value: " .. 42 .. ", Double: " .. (42 * 2) .. "!"
print("Mixed concat:", mixed_result) 
assert(mixed_result == "Value: 42, Double: 84!", "Mixed type concatenation failed")

-- Test 3: Concatenation with metamethods
local custom_obj = {text = "Custom"}
local custom_mt = {}
custom_mt.__concat = function(a, b)
  if type(a) == "table" and a.text then
    return a.text .. tostring(b)
  else
    return tostring(a) .. b.text
  end
end

setmetatable(custom_obj, custom_mt)

-- Metamethod concatenation
local meta_result1 = custom_obj .. " Object"
print("Metamethod concat 1:", meta_result1)
assert(meta_result1 == "Custom Object", "Metamethod concatenation failed")

local meta_result2 = "Prefix " .. custom_obj
print("Metamethod concat 2:", meta_result2) 
assert(meta_result2 == "Prefix Custom", "Reverse metamethod concatenation failed")

-- Test 4: Complex multi-step concatenation with metamethods
local step_result = "Start" .. custom_obj .. " Middle" .. custom_obj .. " End"
print("Complex metamethod concat:", step_result)
assert(step_result == "StartCustom MiddleCustom End", "Complex metamethod concatenation failed")

return "Complex concatenation test passed"