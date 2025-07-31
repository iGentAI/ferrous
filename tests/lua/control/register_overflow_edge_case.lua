-- Register Overflow Edge Case Test
-- This test specifically targets register addressing calculation errors
-- revealed by stack truncation fixes in function call sequences

local function test()
  return true
end

-- Statement context call (should preserve function identity)
test()

-- Expression context call (should work after statement call)
local result = test()

assert(result == true, "Register overflow prevented proper function execution")

return "Register overflow edge case test passed"
