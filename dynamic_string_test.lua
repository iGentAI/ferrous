-- Test dynamic string creation and lookup
local key_part1 = "pr"
local key_part2 = "int"
local dynamic_key = key_part1 .. key_part2  -- Creates "print" dynamically

print("Dynamic key created:", dynamic_key)

-- Try to access the function through dynamic lookup
local func = _G[dynamic_key]

if func == nil then
  print("FAILURE: Dynamic key lookup returned nil")
  return false
else
  print("SUCCESS: Dynamic key lookup works!")
  func("This proves dynamic lookup works")
  return true
end
