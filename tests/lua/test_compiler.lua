-- Minimal test file for the compiler
local x = 42
local y = "hello"

-- Very simple function
local function add(a, b)
  local result = a + b
  return result
end

-- Simple if
if x > 0 then
  print("positive")
end

return x, y, add(1, 2)