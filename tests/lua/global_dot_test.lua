-- Minimal _G access test in dot notation

print("Testing globals dot access...")

-- Access print directly
print("Direct print access works")

-- Define a function that uses dot notation to access _G
local function test_globals_dot()
  print("Inside function, accessing _G.print...")
  local global_print = _G.print -- Using dot notation
  
  if global_print then
    global_print("SUCCESS: _G.print found via dot notation")
    return true
  else
    print("ERROR: _G.print via dot notation is nil!")
    return false
  end
end

-- Run test
local result = test_globals_dot()
print("Test result:", result)

return result
