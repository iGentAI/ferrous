-- Test script to isolate global variable access issue

print("Testing global access...")

-- Directly access print function
print("Direct print access works")

-- Access via _G
print("_G['print'] access:", _G["print"] ~= nil)

-- Function that accesses _G
local function test_globals()
  print("Inside function, accessing _G['print']...")
  local global_print = _G["print"]
  
  if global_print == nil then
    print("ERROR: global_print is nil!")
    return false
  else
    print("SUCCESS: global_print found")
    global_print("Using global_print inside function")
    return true
  end
end

-- Run test
local result = test_globals()
print("Test result:", result)

-- Return test result
return result
